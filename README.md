# apalis-amqp

Message queuing utilities for Rust using apalis and AMQP.

## Overview

`apalis-amqp` is a Rust crate that provides utilities for integrating `apalis` with AMQP message queuing systems. It includes an `AmqpBackend` implementation for use with the pushing and popping jobs, as well as a `AmqpStream` type for consuming messages from an AMQP queue and passing them to `Worker` for processing.

## Features

- Integration between apalis and AMQP message queuing systems.
- Easy creation of AMQP-backed job queues.
- Simple consumption of AMQP messages as apalis jobs.
- Supports message acknowledgement and rejection via `tower` layers.

## Getting started

Add apalis-amqp to your Cargo.toml file:

````toml
[dependencies]
apalis = "0.4.0-alpha.5"
apalis-amqp = "0.1"
serde = "1"
````

Then add to your main.rs

````rust
use apalis::prelude::*;
use apalis_amqp::AmqpBackend;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestJob;

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
    let amqp_backend = AmqpBackend::<TestJob>::new_from_addr(&env).await.unwrap();
    let _queue = amqp_backend.connect().await.unwrap();
    amqp_backend.push_job(TestJob(42)).await.unwrap();
    Monitor::new()
        .register(
            WorkerBuilder::new("rango-amigo")
                .with_stream(|worker| amqp_backend.consume(worker.clone()))
                .build_fn(test_job),
        )
        .run()
        .await
        .unwrap();
}

````

## License

apalis-amqp is licensed under the Apache license. See the LICENSE file for details.


