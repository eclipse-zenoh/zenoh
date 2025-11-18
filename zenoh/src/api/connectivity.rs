use crate::Session;

pub struct Connectivity<'a> {
    pub(crate) session: &'a Session,
}
