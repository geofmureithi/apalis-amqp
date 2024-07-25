use apalis_core::layers::Ack;
use lapin::{options::BasicAckOptions, Error};
use serde::{de::DeserializeOwned, Serialize};

use crate::{utils::Context, AmqpBackend};

impl<M: Send + Sync + Serialize + DeserializeOwned + 'static, Res: Send + Sync + 'static>
    Ack<M, Res> for AmqpBackend<M>
{
    type AckError = Error;
    type Context = Context;
    async fn ack(
        &mut self,
        ctx: &Context,
        _res: &Result<Res, apalis_core::error::Error>,
    ) -> Result<(), Error> {
        self.channel
            .basic_ack(ctx.tag().value(), BasicAckOptions::default())
            .await
    }
}
