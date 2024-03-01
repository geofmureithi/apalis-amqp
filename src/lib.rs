//! # apalis-amqp
//!
//! Message queuing utilities for Rust using apalis and AMQP.

//! ## Overview

//! `apalis-amqp` is a Rust crate that provides utilities for integrating `apalis` with AMQP message queuing systems.
//!  It includes an `AmqpBackend` implementation for use with the pushing and popping jobs, as well as a `MessageQueue<J>` implementation for consuming messages from an AMQP queue and passing them to `Worker` for processing.

//! ## Features

//! - Integration between apalis and AMQP message queuing systems.
//! - Easy creation of AMQP-backed job queues.
//! - Simple consumption of AMQP messages as apalis jobs.
//! - Supports message acknowledgement and rejection via `tower` layers.
//! - Supports all apalis middleware such as rate-limiting, timeouts, filtering, sentry, prometheus etc.

//! ## Getting started

//! Add apalis-amqp to your Cargo.toml file:

//! ````toml
//! [dependencies]
//! apalis = { version = "0.5", features = ["tokio-comp"] }
//! apalis-amqp = "0.3"
//! serde = "1"
//! ````

//! Then add to your main.rs

//! ````rust,no_run
//! use apalis::prelude::*;
//! use apalis_amqp::AmqpBackend;
//! use serde::{Deserialize, Serialize};

//! #[derive(Debug, Serialize, Deserialize)]
//! struct TestJob(usize);

//! impl Message for TestJob {
//!     const NAME: &'static str = "TestJob";
//! }

//! async fn test_job(job: TestJob) {
//!     dbg!(job);
//! }
//!
//! #[derive(Clone, Debug, Default)]
//! pub struct TokioExecutor;
//!
//! impl apalis_core::executor::Executor for TokioExecutor {
//!   fn spawn(&self, future: impl std::future::Future<Output = ()> + Send + 'static) {
//!        tokio::spawn(future);
//!    }
//! }
//! #[tokio::main]
//! async fn main() {
//!     let env = std::env::var("AMQP_ADDR").unwrap();
//!     let mq = AmqpBackend::<TestJob>::new_from_addr(&env).await.unwrap();
//!     mq.enqueue(TestJob(42)).await.unwrap();
//!     Monitor::<TokioExecutor>::new()
//!         .register(
//!             WorkerBuilder::new("rango-amigo")
//!                 .with_mq(mq)
//!                 .build_fn(test_job),
//!         )
//!         .run()
//!         .await
//!         .unwrap();
//! }

//! ````
#![forbid(unsafe_code)]
#![warn(
    clippy::await_holding_lock,
    clippy::cargo_common_metadata,
    clippy::dbg_macro,
    clippy::empty_enum,
    clippy::enum_glob_use,
    clippy::inefficient_to_string,
    clippy::mem_forget,
    clippy::mutex_integer,
    clippy::needless_continue,
    clippy::todo,
    clippy::unimplemented,
    clippy::wildcard_imports,
    future_incompatible,
    missing_docs,
    missing_debug_implementations,
    unreachable_pub
)]

mod ack;

use apalis_core::{
    mq::{Message, MessageQueue},
    poller::Poller,
    request::{Request, RequestStream},
    worker::WorkerId,
    Backend,
};
use deadpool_lapin::{Manager, Pool};
use futures::StreamExt;
use lapin::{
    options::{BasicConsumeOptions, BasicPublishOptions, QueueDeclareOptions},
    types::FieldTable,
    BasicProperties, Channel, ConnectionProperties, Error, Queue,
};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fmt::Debug,
    io::{self, ErrorKind},
    marker::PhantomData,
    sync::Arc,
};
use tower::{layer::util::Identity, ServiceBuilder};

#[derive(Debug)]
/// A wrapper around a `lapin` AMQP channel that implements message queuing functionality.
pub struct AmqpBackend<M> {
    channel: Channel,
    queue: Queue,
    job_type: PhantomData<M>,
}

impl<J> Clone for AmqpBackend<J> {
    fn clone(&self) -> Self {
        Self {
            channel: self.channel.clone(),
            queue: self.queue.clone(),
            job_type: PhantomData,
        }
    }
}

impl<M: Message + Serialize + DeserializeOwned + Send + Sync + 'static> MessageQueue<M>
    for AmqpBackend<M>
{
    type Error = Error;
    /// Publishes a new job to the queue.
    ///
    /// This function serializes the provided job data to a JSON string and publishes it to the
    /// queue with the name of the job type `J::NAME`.
    async fn enqueue(&self, data: M) -> Result<(), Self::Error> {
        let channel = self.channel.clone();

        let _confirmation = channel
            .basic_publish(
                "",
                M::NAME,
                BasicPublishOptions::default(),
                &serde_json::to_vec(&Request::new(data)).map_err(|e| {
                    Error::IOError(Arc::new(io::Error::new(ErrorKind::InvalidData, e)))
                })?,
                BasicProperties::default(),
            )
            .await?
            .await?;
        Ok(())
    }

    async fn size(&self) -> Result<usize, Self::Error> {
        todo!()
    }

    async fn dequeue(&self) -> Result<Option<M>, Self::Error> {
        Ok(None)
    }
}

impl<M: DeserializeOwned + Message + Send + 'static> Backend<Request<M>> for AmqpBackend<M> {
    type Layer = ServiceBuilder<Identity>;
    type Stream = RequestStream<Request<M>>;
    fn common_layer(&self, _worker: WorkerId) -> Self::Layer {
        ServiceBuilder::new()
    }

    fn poll(self, worker: WorkerId) -> Poller<Self::Stream> {
        let channel = self.channel.clone();
        let worker = worker.clone();
        let stream = async_stream::stream! {
            let mut consumer = channel
            .basic_consume(
                M::NAME,
                &worker.to_string(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| apalis_core::error::Error::SourceError(e.into()))?;

            while let Some(Ok(item)) = consumer.next().await {
                let bytes = item.data;
                let mut job: Request<M> = serde_json::from_slice(&bytes)
                    .map_err(|e| apalis_core::error::Error::SourceError(e.into()))?;
                job.insert(DeliveryTag(item.delivery_tag)); // requires extensions
                yield Ok(Some(job));

            }
        };
        Poller::new(stream.boxed(), async {})
    }
}

#[derive(Debug, Clone)]
/// A wrapper for the the job to be acknowledged.
pub struct DeliveryTag(u64);

impl<M: Message + Serialize + DeserializeOwned + Send + 'static> AmqpBackend<M> {
    /// Constructs a new instance of `AmqpBackend` from a `lapin` channel.
    pub fn new(channel: Channel, queue: Queue) -> Self {
        Self {
            channel,
            job_type: PhantomData,
            queue,
        }
    }

    /// Get a ref to the inner `Channel`
    pub fn channel(&self) -> &Channel {
        &self.channel
    }

    /// Get a ref to the inner `Queue`
    pub fn queue(&self) -> &Queue {
        &self.queue
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
        let queue = channel
            .queue_declare(
                M::NAME,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;
        Ok(Self::new(channel, queue))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apalis_core::{builder::WorkerBuilder, builder::WorkerFactoryFn};
    use serde::Deserialize;

    #[derive(Debug, Serialize, Deserialize)]
    struct TestMessage;

    impl Message for TestMessage {
        const NAME: &'static str = "TestMessage";
    }

    async fn test_job(_job: TestMessage) {}

    #[tokio::test]
    async fn it_works() {
        let env = std::env::var("AMQP_ADDR").unwrap();
        let amqp_backend = AmqpBackend::<TestMessage>::new_from_addr(&env)
            .await
            .unwrap();
        let _worker = WorkerBuilder::new("rango-amigo")
            .with_mq(amqp_backend)
            .build_fn(test_job);
    }
}
