use apalis::{layers::retry::RetryPolicy, prelude::*};
use apalis_amqp::AmqpBackend;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestMessage(usize);

async fn test_job(job: TestMessage, count: Data<usize>) {
    dbg!(job);
    dbg!(count);
}

#[tokio::main]
async fn main() {
    let env = std::env::var("AMQP_ADDR").unwrap();
    let mut mq = AmqpBackend::new_from_addr(&env).await.unwrap();
    // add some jobs
    mq.enqueue(TestMessage(42)).await.unwrap();
    Monitor::new()
        .register({
            WorkerBuilder::new("rango-amigo")
                .data(0usize)
                .retry(RetryPolicy::retries(5))
                .backend(mq)
                .build_fn(test_job)
        })
        .run()
        .await
        .unwrap();
}
