use apalis_core::{layers::Ack, mq::Message, worker::WorkerId};
use lapin::{options::BasicAckOptions, Error};
use serde::{de::DeserializeOwned, Serialize};

use crate::{AmqpBackend, DeliveryTag};

impl<M: Send + Sync + Serialize + DeserializeOwned + Message + 'static> Ack<M> for AmqpBackend<M> {
    type Error = Error;
    type Acknowledger = DeliveryTag;
    async fn ack(&self, _worker_id: &WorkerId, tag: &DeliveryTag) -> Result<(), Error> {
        let channel = self.channel().clone();
        channel.basic_ack(tag.0, BasicAckOptions::default()).await
    }
}
