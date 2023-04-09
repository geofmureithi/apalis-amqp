use apalis_core::{
    context::{HasJobContext, JobContext},
    job::{Job, JobStreamResult},
    request::JobRequest,
    worker::WorkerId,
};
use deadpool_lapin::{Manager, Pool};
use futures::{future::BoxFuture, StreamExt};
use lapin::{
    options::{BasicAckOptions, BasicConsumeOptions, BasicPublishOptions, QueueDeclareOptions},
    publisher_confirm::Confirmation,
    types::FieldTable,
    BasicProperties, Channel, ConnectionProperties, Queue,
};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fmt::Debug,
    marker::PhantomData,
    task::{Context, Poll},
};
use tower::Service;

#[derive(Debug)]
/// A wrapper around a `lapin` AMQP channel that implements message queuing functionality.
pub struct AmqpBackend<J>(Channel, PhantomData<J>);

/// A stream of jobs produced
pub type AmqpStream<J> = JobStreamResult<J>;

#[derive(Debug, Clone)]
/// A wrapper for the the job to be acknowledged.
pub struct DeliveryTag(u64);

impl<J: Job + Serialize + DeserializeOwned + Send + 'static> AmqpBackend<J> {
    /// Constructs a new instance of `AmqpBackend` from a `lapin` channel.
    pub fn new(channel: Channel) -> Self {
        Self(channel, PhantomData)
    }

    /// Get a ref to the inner `Channel`
    pub fn channel(&self) -> &Channel {
        &self.0
    }

    /// Constructs a new instance of `AmqpBackend` from an address string.
    ///
    /// This function creates a `deadpool_lapin::Pool` and uses it to obtain a `lapin::Connection`.
    /// It then creates a channel from that connection.
    pub async fn new_from_addr<S: AsRef<str>>(addr: S) -> Result<Self, lapin::Error> {
        let manager = Manager::new(addr.as_ref(), ConnectionProperties::default());
        let pool: Pool = deadpool::managed::Pool::builder(manager)
            .max_size(10)
            .build()
            .expect("can create pool");
        let amqp_conn = pool
            .get()
            .await
            .map_err(|_e| lapin::Error::ChannelsLimitReached)?;
        let channel = amqp_conn.create_channel().await?;
        Ok(Self(channel, PhantomData))
    }

    /// Declares a queue on the channel with the name of the job type `J::NAME`.
    ///
    /// Returns a `Queue` object representing the declared queue.
    pub async fn connect(&self) -> Result<Queue, lapin::Error> {
        let queue = self
            .0
            .queue_declare(
                J::NAME,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;
        Ok(queue)
    }

    /// Consumes jobs from the queue and returns a `BoxStream` of `JobRequest<J>` objects.
    ///
    /// This function creates a new asynchronous stream using the `async_stream` crate that
    /// continuously consumes messages from the queue and converts them to `JobRequest<J>` objects
    /// using `serde_json::from_slice`.

    pub fn consume(&self, worker: WorkerId) -> AmqpStream<J> {
        let channel = self.0.clone();
        let stream = async_stream::stream! {
            let mut consumer = channel
            .basic_consume(
                J::NAME,
                &worker.to_string(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await.unwrap();
            while let Some(Ok(item)) = consumer.next().await {
                let bytes = item.data;
                let mut job: JobRequest<J> = serde_json::from_slice(&bytes).unwrap();
                job.insert(DeliveryTag(item.delivery_tag));
                yield Ok(Some(job));

            }
        };
        stream.boxed()
    }

    /// Publishes a new job to the queue.
    ///
    /// This function serializes the provided job data to a JSON string and publishes it to the
    /// queue with the name of the job type `J::NAME`.
    pub async fn push_job(&self, data: J) -> Result<Confirmation, lapin::Error> {
        let channel = self.0.clone();

        let confirmation = channel
            .basic_publish(
                "",
                J::NAME,
                BasicPublishOptions::default(),
                &serde_json::to_vec(&JobRequest::new(data))
                    .map_err(|_e| lapin::Error::ChannelsLimitReached)?,
                BasicProperties::default(),
            )
            .await?
            .await?;
        Ok(confirmation)
    }
}

/// A middleware that acknowledges a job the forwards it to another service
pub struct AckService<S> {
    channel: Channel,
    service: S,
}

impl<S> AckService<S> {
    pub fn new(channel: Channel, service: S) -> Self {
        Self { channel, service }
    }
}

impl<S, Request> Service<Request> for AckService<S>
where
    S: Service<Request>,
    Request: HasJobContext,
    <S as Service<Request>>::Response: std::marker::Send,
    <S as Service<Request>>::Error: std::marker::Send + Debug,
    <S as Service<Request>>::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<
        'static,
        Result<<S as Service<Request>>::Response, <S as Service<Request>>::Error>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let ctx: &JobContext = &request.context();
        let tag = ctx.data_opt::<DeliveryTag>().cloned().unwrap();
        let channel = self.channel.clone();
        let fut = self.service.call(request);

        Box::pin(async move {
            let res = fut.await;
            match channel.basic_ack(tag.0, BasicAckOptions::default()).await {
                Ok(_) => tracing::trace!("Successfully acknowledged job"),
                Err(e) => tracing::debug!("An error occurred {e}"),
            }
            res
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apalis_core::builder::WorkerBuilder;
    use apalis_core::builder::WorkerFactoryFn;
    use serde::Deserialize;
    use tower::layer::layer_fn;

    #[derive(Debug, Serialize, Deserialize)]
    struct TestJob;

    impl Job for TestJob {
        const NAME: &'static str = "TestJob";
    }

    async fn test_job(_job: TestJob, _ctx: JobContext) {}

    #[tokio::test]
    async fn it_works() {
        let env = std::env::var("AMQP_ADDR").unwrap();
        let amqp_backend = AmqpBackend::<TestJob>::new_from_addr(&env).await.unwrap();
        let _res = amqp_backend.connect().await.unwrap();

        let _worker = WorkerBuilder::new("rango-amigo")
            .layer(layer_fn(|service| {
                AckService::new(amqp_backend.channel().clone(), service)
            }))
            .with_stream(|worker| amqp_backend.consume(worker.clone()))
            .build_fn(test_job);
    }
}
