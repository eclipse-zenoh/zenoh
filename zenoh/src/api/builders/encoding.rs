use crate::api::encoding::{AutoEncoding, Encoding};

pub trait EncodingBuilderTrait {
    /// Set the [`Encoding`]
    fn encoding<T: Into<Encoding>>(self, encoding: T) -> Self;
}

pub trait AutoEncodingBuilderTrait<Payload>: EncodingBuilderTrait + Sized
where
    Payload: AutoEncoding,
{
    /// Set the [`Encoding`] according to the payload type
    fn auto_encoding(self) -> Self {
        if let Some(payload) = self.get_payload() {
            let encoding = payload.get_encoding();
            return self.encoding(encoding);
        }
        self
    }
    #[doc(hidden)]
    fn get_payload(&self) -> Option<&Payload>;
}
