use apalis_amqp::AmqpBackend;
use apalis_core::builder::WorkerFactoryFn;
use apalis_core::mq::MessageQueue;
use apalis_core::{builder::WorkerBuilder, layers::extensions::Data, monitor::Monitor};
use serde::{Deserialize, Serialize};
use tower::retry::RetryLayer;

mod policy;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestMessage(usize);

async fn test_job(job: TestMessage, count: Data<usize>) {
    dbg!(job);
    dbg!(count);
}

#[derive(Clone, Debug, Default)]
pub struct TokioExecutor;

impl apalis_core::executor::Executor for TokioExecutor {
    fn spawn(&self, future: impl std::future::Future<Output = ()> + Send + 'static) {
        tokio::spawn(future);
    }
}

#[tokio::main]
async fn main() {
    let env = std::env::var("AMQP_ADDR").unwrap();
    let mut mq = AmqpBackend::new_from_addr(&env).await.unwrap();
    // add some jobs
    mq.enqueue(TestMessage(42)).await.unwrap();
    Monitor::<TokioExecutor>::new()
        .register_with_count(3, {
            WorkerBuilder::new(format!("rango-amigo"))
                .data(0usize)
                .layer(RetryLayer::new(policy::RetryPolicy::retries(5)))
                .backend(mq)
                .build_fn(test_job)
        })
        .run()
        .await
        .unwrap();
}
