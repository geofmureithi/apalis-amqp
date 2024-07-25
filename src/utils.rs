use apalis_core::data::Extensions;
use apalis_core::request::Request;
use apalis_core::task::attempt::Attempt;
use apalis_core::task::namespace::Namespace;
use apalis_core::task::task_id::TaskId;
use serde::{Deserialize, Serialize};

/// Config for the backend
#[derive(Clone, Debug)]
pub struct Config {
    max_retries: usize,
    namespace: Namespace,
}

impl Config {
    /// Creates a new `Config` instance with the given namespace.
    pub fn new(namespace: &str) -> Self {
        Config {
            max_retries: 25,
            namespace: namespace.to_owned().into(),
        }
    }

    /// Gets the maximum number of retries.
    pub fn max_retries(&self) -> usize {
        self.max_retries
    }

    /// Sets the maximum number of retries.
    pub fn set_max_retries(&mut self, max_retries: usize) {
        self.max_retries = max_retries;
    }

    /// Gets the namespace.
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }

    /// Sets the namespace.
    pub fn set_namespace(&mut self, namespace: Namespace) {
        self.namespace = namespace;
    }
}

/// The context of a message
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Context {
    task_id: TaskId,
    tag: DeliveryTag,
    attempt: Attempt,
}

impl Context {
    /// Creates a new `Context` instance with the given parameters.
    pub fn new(task_id: TaskId, tag: DeliveryTag, attempt: Attempt) -> Self {
        Context {
            task_id,
            tag,
            attempt,
        }
    }

    /// Gets the task ID.
    pub fn task_id(&self) -> &TaskId {
        &self.task_id
    }

    /// Sets the task ID.
    pub fn set_task_id(&mut self, task_id: TaskId) {
        self.task_id = task_id;
    }

    /// Gets the delivery tag.
    pub fn tag(&self) -> &DeliveryTag {
        &self.tag
    }

    /// Sets the delivery tag.
    pub fn set_tag(&mut self, tag: DeliveryTag) {
        self.tag = tag;
    }

    /// Gets the attempt.
    pub fn attempt(&self) -> &Attempt {
        &self.attempt
    }

    /// Sets the attempt.
    pub fn set_attempt(&mut self, attempt: Attempt) {
        self.attempt = attempt;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
/// A wrapper for the message to be acknowledged.
pub struct DeliveryTag(u64);

impl DeliveryTag {
    /// Creates a new `DeliveryTag` instance with the given value.
    pub fn new(value: u64) -> Self {
        DeliveryTag(value)
    }

    /// Gets the delivery tag value.
    pub fn value(&self) -> u64 {
        self.0
    }

    /// Sets the delivery tag value.
    pub fn set_value(&mut self, value: u64) {
        self.0 = value;
    }
}

/// The representation that is sent in the underlying connection
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AmqpMessage<T> {
    msg: T,
    ctx: Context,
}

impl<T> AmqpMessage<T> {
    /// Creates a new `AmqpMessage` instance with the given message and context.
    pub fn new(msg: T, ctx: Context) -> Self {
        AmqpMessage { msg, ctx }
    }

    /// Gets the message.
    pub fn msg(&self) -> &T {
        &self.msg
    }

    /// Sets the message.
    pub fn set_msg(&mut self, msg: T) {
        self.msg = msg;
    }

    /// Gets the context.
    pub fn ctx(&self) -> &Context {
        &self.ctx
    }

    /// Sets the context.
    pub fn set_ctx(&mut self, ctx: Context) {
        self.ctx = ctx;
    }
}

impl<T> Into<Request<T>> for AmqpMessage<T> {
    fn into(self) -> Request<T> {
        let mut data = Extensions::default();
        data.insert(self.ctx);
        Request::new_with_data(self.msg, data)
    }
}
