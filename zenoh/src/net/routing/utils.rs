use zenoh_protocol::network::declare::queryable::ext::QueryableInfoType;

pub(crate) fn iter_if<I: IntoIterator>(
    cond: bool,
    iter: impl FnOnce() -> I,
) -> impl Iterator<Item = I::Item> {
    cond.then(iter).into_iter().flatten()
}

pub(crate) fn merge_queryable_infos(
    infos: impl IntoIterator<Item = QueryableInfoType>,
) -> Option<QueryableInfoType> {
    infos.into_iter().fold(None, |accu, info| match accu {
        Some(accu) => Some(QueryableInfoType {
            complete: accu.complete || info.complete,
            distance: std::cmp::min(accu.distance, info.distance),
        }),
        None => Some(info),
    })
}
