<h1 align="center">apalis-amqp</h1>
<div align="center">
 <strong>
   Message queuing for Rust using apalis and AMQP.
 </strong>
</div>

<br />

<div align="center">
  <!-- Crates version -->
  <a href="https://crates.io/crates/apalis-amqp">
    <img src="https://img.shields.io/crates/v/apalis-amqp.svg?style=flat-square"
    alt="Crates.io version" />
  </a>
  <!-- Downloads -->
  <a href="https://crates.io/crates/apalis-amqp">
    <img src="https://img.shields.io/crates/d/apalis-amqp.svg?style=flat-square"
      alt="Download" />
  </a>
  <!-- docs.rs docs -->
  <a href="https://docs.rs/apalis-amqp">
    <img src="https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square"
      alt="docs.rs docs" />
  </a>
</div>
<br/>

## Overview

`apalis-amqp` is a Rust crate that provides utilities for integrating `apalis` with AMQP message queuing systems. It includes an `AmqpBackend` implementation for use with the pushing and popping jobs, as well as a `MessageQueue<J>` implementation for consuming messages from an AMQP queue and passing them to `ReadyWorker` for processing.

## Features

- Integration between apalis and AMQP message queuing systems.
- Easy creation of AMQP-backed job queues.
- Simple consumption of AMQP messages as apalis jobs.
- Supports message acknowledgement and rejection via `tower` layers.
- Supports all apalis middleware such as rate-limiting, timeouts, filtering, sentry, prometheus etc.

## Getting started

Add apalis-amqp to your Cargo.toml file:

### Setup RabbitMq

```
docker run -p 15672:15672 -p 5672:5672 -e RABBITMQ_DEFAULT_USER=apalis -e RABBITMQ_DEFAULT_PASS=apalis  rabbitmq:3.8.4-management
```

### Setup the rust code

````toml
[dependencies]
apalis = "0.4"
apalis-amqp = "0.2"
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
    let mq = AmqpBackend::<TestJob>::new_from_addr(&env).await.unwrap();
    mq.push(TestJob(42)).await.unwrap();
    Monitor::new()
        .register(
            WorkerBuilder::new("rango-amigo")
                .with_mq(mq)
                .build_fn(test_job),
        )
        .run()
        .await
        .unwrap();
}

````

## License

apalis-amqp is licensed under the Apache license. See the LICENSE file for details.


