use apalis_amqp::AmqpBackend;
use apalis_core::{
    builder::WorkerBuilder,
    builder::WorkerFactoryFn,
    context::JobContext,
    job::Job,
    monitor::Monitor,
    mq::{MessageQueue, WithMq}
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestJob(usize);

impl Job for TestJob {
    const NAME: &'static str = "TestJob";
}

async fn test_job(job: TestJob, ctx: JobContext) {
    dbg!(job);
    dbg!(ctx);
}

#[tokio::main]
async fn main() {
    let env = std::env::var("AMQP_ADDR").unwrap();
    let mq = AmqpBackend::<TestJob>::new_from_addr(&env).await.unwrap();
    // add some jobs
    mq.push(TestJob(42)).await.unwrap();
    Monitor::new()
        .register_with_count(3, |index| {
            WorkerBuilder::new(format!("rango-amigo-{index}"))
                .with_mq(mq.clone())
                .build_fn(test_job)
        })
        .run()
        .await
        .unwrap();
}
