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
//! apalis = { version = "0.6.0-rc.5", features = ["tokio-comp"] }
//! apalis-amqp = "0.4"
//! serde = "1"
//! ````

//! Then add to your main.rs

//! ````rust,no_run
//! use apalis::prelude::*;
//! use apalis_amqp::AmqpBackend;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! struct TestJob(usize);
//!
//! async fn test_job(job: TestJob) {
//!     dbg!(job);
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let env = std::env::var("AMQP_ADDR").unwrap();
//!     let mq = AmqpBackend::<TestJob>::new_from_addr(&env).await.unwrap();
//!     mq.enqueue(TestJob(42)).await.unwrap();
//!     Monitor::<TokioExecutor>::new()
//!         .register(
//!             WorkerBuilder::new("rango-amigo")
//!                 .backend(mq)
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
    mq::MessageQueue,
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
use tower::layer::util::Identity;
use utils::{AmqpMessage, Config};

/// Contains basic utilities for handling config and messages
pub mod utils;

#[derive(Debug)]
/// A wrapper around a `lapin` AMQP channel that implements message queuing functionality.
pub struct AmqpBackend<M> {
    channel: Channel,
    queue: Queue,
    job_type: PhantomData<M>,
    config: Config,
}

impl<J> Clone for AmqpBackend<J> {
    fn clone(&self) -> Self {
        Self {
            channel: self.channel.clone(),
            queue: self.queue.clone(),
            job_type: PhantomData,
            config: self.config.clone(),
        }
    }
}

impl<M: Serialize + DeserializeOwned + Send + Sync + 'static> MessageQueue<M> for AmqpBackend<M> {
    type Error = Error;
    /// Publishes a new job to the queue.
    ///
    /// This function serializes the provided job data to a JSON string and publishes it to the
    /// queue with the namespace configured.
    async fn enqueue(&mut self, message: M) -> Result<(), Self::Error> {


        let _confirmation = self
            .channel
            .basic_publish(
                "",
                self.config.namespace().as_str(),
                BasicPublishOptions::default(),
                &serde_json::to_vec(&Request::new(message)).map_err(|e| {
                    Error::IOError(Arc::new(io::Error::new(ErrorKind::InvalidData, e)))
                })?,
                BasicProperties::default(),
            )
            .await?
            .await?;
        Ok(())
    }

    async fn size(&mut self) -> Result<usize, Self::Error> {
        todo!()
    }

    async fn dequeue(&mut self) -> Result<Option<M>, Self::Error> {
        Ok(None)
    }
}

impl<M: DeserializeOwned + Send + 'static, Res> Backend<Request<M>, Res> for AmqpBackend<M> {
    type Layer = Identity;
    type Stream = RequestStream<Request<M>>;

    fn poll<Svc>(self, worker: WorkerId) -> Poller<Self::Stream> {
        let channel = self.channel.clone();
        let worker = worker.clone();
        let stream = async_stream::stream! {
            let mut consumer = channel
            .basic_consume(
                self.config.namespace().as_str(),
                &worker.to_string(),
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(|e| apalis_core::error::Error::SourceError(Arc::new(e.into())))?;

            while let Some(Ok(item)) = consumer.next().await {
                let bytes = item.data;
                let msg: AmqpMessage<M> = serde_json::from_slice(&bytes)
                    .map_err(|e| apalis_core::error::Error::SourceError(Arc::new(e.into())))?;
                yield Ok(Some(msg.into()));

            }
        };
        Poller::new(stream.boxed(), async {})
    }
}

impl<M: Serialize + DeserializeOwned + Send + 'static> AmqpBackend<M> {
    /// Constructs a new instance of `AmqpBackend` from a `lapin` channel.
    pub fn new(channel: Channel, queue: Queue) -> Self {
        Self {
            channel,
            job_type: PhantomData,
            queue,
            config: Config::new(std::any::type_name::<M>()),
        }
    }

    /// Constructs a new instance of `AmqpBackend` with a config
    pub fn new_with_config(channel: Channel, queue: Queue, config: Config) -> Self {
        Self {
            channel,
            job_type: PhantomData,
            queue,
            config,
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

    /// Get a ref to the inner `Config`
    pub fn config(&self) -> &Config {
        &self.config
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
            .map_err(|error| {
                lapin::Error::IOError(Arc::new(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    error,
                )))
            })?;
        let amqp_conn = pool.get().await.map_err(|error| {
            lapin::Error::IOError(Arc::new(io::Error::new(
                io::ErrorKind::ConnectionRefused,
                error,
            )))
        })?;
        let channel = amqp_conn.create_channel().await?;
        let queue = channel
            .queue_declare(
                std::any::type_name::<M>(),
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

    async fn test_job(_job: TestMessage) {}

    #[tokio::test]
    async fn it_works() {
        let env = std::env::var("AMQP_ADDR").unwrap();
        let amqp_backend = AmqpBackend::<TestMessage>::new_from_addr(&env)
            .await
            .unwrap();
        let _worker = WorkerBuilder::new("rango-amigo")
            .backend(amqp_backend)
            .build_fn(test_job);
    }
}
