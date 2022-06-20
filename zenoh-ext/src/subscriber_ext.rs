use flume::r#async::RecvStream;
use futures::stream::{Forward, Map};
use zenoh::{prelude::Sample, subscriber::HandlerSubscriber};

pub trait HandlerSubscriberForward<'a, S> {
    type Output;
    fn forward(&'a mut self, sink: S) -> Self::Output;
}
impl<'a, S> HandlerSubscriberForward<'a, S> for HandlerSubscriber<'_, flume::Receiver<Sample>>
where
    S: futures::sink::Sink<Sample>,
{
    type Output = Forward<Map<RecvStream<'a, Sample>, fn(Sample) -> Result<Sample, S::Error>>, S>;
    fn forward(&'a mut self, sink: S) -> Self::Output {
        futures::StreamExt::forward(futures::StreamExt::map(self.receiver.stream(), Ok), sink)
    }
}
