use apalis_core::{layers::Ack, response::Response};
use lapin::{options::BasicAckOptions, Error};
use serde::{de::DeserializeOwned, Serialize};

use crate::{utils::AmqpContext, AmqpBackend};

impl<M: Send + Sync + Serialize + DeserializeOwned + 'static, Res: Send + Sync + 'static>
    Ack<M, Res> for AmqpBackend<M>
{
    type AckError = Error;
    type Context = AmqpContext;
    async fn ack(&mut self, ctx: &AmqpContext, _res: &Response<Res>) -> Result<(), Error> {
        self.channel
            .basic_ack(ctx.tag().value(), BasicAckOptions::default())
            .await
    }
}
