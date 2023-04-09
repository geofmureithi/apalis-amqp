# apalis-amqp

Message queuing utilities for Rust using apalis and AMQP.

## Overview

`apalis-amqp` is a Rust crate that provides utilities for integrating `apalis` with AMQP message queuing systems. It includes an `AmqpBackend` implementation for use with the pushing and popping jobs, as well as a `AmqpStream` type for consuming messages from an AMQP queue and passing them to `Worker` for processing.

## Features

- Integration between apalis and AMQP message queuing systems.
- Easy creation of AMQP-backed job queues.
- Simple consumption of AMQP messages as apalis jobs.
- Supports message acknowledgement and rejection.

## Getting started

Add apalis-amqp to your Cargo.toml file:

````toml
[dependencies]
apalis = "0.4"
apalis-amqp = "0.1"
````

Then add to your main.rs

````rust

#[tokio::main]
async fn main() -> Result<()> {
    let env = std::env::var("AMQP_ADDR")?;
    let amqp_backend = AmqpBackend::<TestJob>::new_from_addr(&env).await?;
    let queue = amqp_backend.connect().await?;
    Monitor::new()
        .register(
            WorkerBuilder::new("rango-amigo")
                .layer(layer_fn(|service| {
                    AckService::new(amqp_backend.channel().clone(), service)
                }))
                .with_stream(|worker| amqp_backend.consume(worker.clone()))
                .build_fn(test_job),
            )
        .run()
        .await
}

## License

apalis-amqp is licensed under the Apache license. See the LICENSE file for details.