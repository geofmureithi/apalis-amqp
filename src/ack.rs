use apalis_core::{
    job::{Job},
    layers::ack::{Ack, AckError},
    worker::WorkerId,
};
use lapin::options::BasicAckOptions;
use serde::{de::DeserializeOwned, Serialize};

use crate::{AmqpBackend, DeliveryTag};

#[async_trait::async_trait]
impl<J: Send + Sync + Serialize + DeserializeOwned + Job + 'static> Ack<J> for AmqpBackend<J> {
    type Acknowledger = DeliveryTag;
    async fn ack(&self, _worker_id: &WorkerId, tag: &DeliveryTag) -> Result<(), AckError> {
        let channel = self.channel().clone();
        channel
            .basic_ack(tag.0, BasicAckOptions::default())
            .await
            .map_err(|e| AckError::NoAck(e.into()))
    }
}
