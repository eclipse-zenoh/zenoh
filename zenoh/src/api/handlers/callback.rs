//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

//! Callback handler trait.

use std::{cell::RefCell, fmt, sync::Arc};

use crate::api::{cancellation::GroupId, handlers::IntoHandler};

thread_local! {
    /// The `SyncGroup`s whose user callbacks are currently on this thread's
    /// stack, innermost last.
    ///
    /// A `Vec` rather than a counter, because the question `SyncGroup::wait`
    /// needs answered is not "am I inside a callback?" but "am I inside a
    /// callback of *this* group?". Only the second is a self-join; the first
    /// also catches the case where the barrier is achievable, and dropping it
    /// there loses a guarantee the API promises.
    ///
    /// Nesting is real — a callback may deliver to another entity — so entries
    /// are pushed and popped rather than set and cleared. Depth is bounded by
    /// the user's own re-entrancy, and the common case is zero or one.
    static EXECUTING_GROUPS: RefCell<Vec<GroupId>> = const { RefCell::new(Vec::new()) };
}

/// Whether a callback belonging to `group` is executing on the current thread.
pub(crate) fn callback_of_group_running_on_this_thread(group: GroupId) -> bool {
    EXECUTING_GROUPS.with_borrow(|groups| groups.contains(&group))
}

/// Pushes a callback's groups for the duration of one invocation.
///
/// The `Drop` is what makes this unwind-safe: a user callback that panics must
/// not leave its groups registered, or every later `wait` on them would take
/// the asynchronous path forever.
struct ExecutingGroups(usize);

impl ExecutingGroups {
    fn enter(groups: &[GroupId]) -> Self {
        EXECUTING_GROUPS.with_borrow_mut(|stack| stack.extend_from_slice(groups));
        Self(groups.len())
    }
}

impl Drop for ExecutingGroups {
    fn drop(&mut self) {
        EXECUTING_GROUPS.with_borrow_mut(|stack| {
            stack.truncate(stack.len().saturating_sub(self.0));
        });
    }
}

/// A function that can transform an [`FnMut`]`(T)` into
/// an [`Fn`]`(T)` with the help of a [`Mutex`](std::sync::Mutex).
pub fn locked<T>(fnmut: impl FnMut(T)) -> impl Fn(T) {
    let lock = std::sync::Mutex::new(fnmut);
    move |x| zlock!(lock)(x)
}

pub trait CallbackParameter: 'static {
    type Message<'a>;

    fn from_message(msg: Self::Message<'_>) -> Self;
}

trait CallbackImpl<T>: Send + Sync {
    fn call(&self, t: T);

    fn call_with_message(&self, msg: T::Message<'_>)
    where
        T: CallbackParameter,
    {
        self.call(T::from_message(msg))
    }
}

impl<T, F: Fn(T) + Send + Sync> CallbackImpl<T> for F {
    fn call(&self, t: T) {
        self(t)
    }
}

struct Dropper<F>
where
    F: FnOnce() + Send + Sync,
{
    drop: Option<F>,
}
impl<F> Drop for Dropper<F>
where
    F: FnOnce() + Send + Sync,
{
    fn drop(&mut self) {
        if let Some(d) = self.drop.take() {
            (d)()
        }
    }
}
trait DropperTrait {}
impl<F> DropperTrait for Dropper<F> where F: FnOnce() + Send + Sync {}
/// Callback type used by zenoh entities.
///
/// This type stores the callback function passed to zenoh entities.
pub struct Callback<T> {
    callable: Arc<dyn CallbackImpl<T>>,
    drop: Option<Arc<dyn DropperTrait + Send + Sync>>,
    /// The `SyncGroup`s this callback holds an on-drop permit in.
    ///
    /// Shared with every clone, because every clone holds the same permits:
    /// the permit is released by the dropper, which runs when the last clone
    /// dies. Empty for a callback that was never registered with a group.
    groups: Arc<[GroupId]>,
}

impl<T> fmt::Debug for Callback<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Callback")
            .field("callable", &"..")
            .field("has_drop", &self.drop.is_some())
            .finish()
    }
}

impl<T> Clone for Callback<T> {
    fn clone(&self) -> Self {
        Self {
            callable: self.callable.clone(),
            drop: self.drop.clone(),
            groups: self.groups.clone(),
        }
    }
}

impl<T> Callback<T> {
    // TODO deprecate
    /// Instantiate a `Callback` from a callback function.
    pub fn new(cb: Arc<dyn Fn(T) + Send + Sync>) -> Self
    where
        T: 'static,
    {
        Self::from(move |msg| cb(msg))
    }

    /// Call the inner callback.
    #[inline]
    pub fn call(&self, arg: T) {
        let _executing = ExecutingGroups::enter(&self.groups);
        self.callable.call(arg)
    }

    pub(crate) fn call_with_message(&self, msg: T::Message<'_>)
    where
        T: CallbackParameter,
    {
        let _executing = ExecutingGroups::enter(&self.groups);
        self.callable.call_with_message(msg)
    }

    #[zenoh_macros::pub_visibility_if_internal]
    pub(crate) fn set_on_drop(&mut self, drop: impl FnOnce() + Send + Sync + 'static) {
        self.drop = Some(Arc::new(Dropper { drop: Some(drop) }));
    }

    /// Records which `SyncGroup`s this callback holds a permit in, so that a
    /// `wait` on one of them can recognise its own callback on this thread.
    pub(crate) fn set_groups(&mut self, groups: impl IntoIterator<Item = GroupId>) {
        self.groups = groups.into_iter().collect();
    }
}

impl<T, F: Fn(T) + Send + Sync + 'static> From<F> for Callback<T> {
    fn from(value: F) -> Self {
        Self {
            callable: Arc::new(value),
            drop: None,
            groups: Arc::from([]),
        }
    }
}

impl<T> IntoHandler<T> for Callback<T> {
    type Handler = ();
    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        (self, ())
    }
}

impl<T, F, H> IntoHandler<T> for (F, H)
where
    F: Fn(T) + Send + Sync + 'static,
{
    type Handler = H;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        (Callback::from(self.0), self.1)
    }
}

impl<T, H> IntoHandler<T> for (Callback<T>, H) {
    type Handler = H;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        self
    }
}

impl<T: Send + 'static> IntoHandler<T> for (flume::Sender<T>, flume::Receiver<T>) {
    type Handler = flume::Receiver<T>;

    fn into_handler(self) -> (Callback<T>, Self::Handler) {
        let (sender, receiver) = self;
        (
            Callback::from(move |t| {
                if let Err(e) = sender.send(t) {
                    tracing::error!("{}", e)
                }
            }),
            receiver,
        )
    }
}

/// A handler containing two callback functions:
///  - `callback`: the typical callback function. `context` will be passed as its last argument.
///  - `drop`: a callback invoked when this handler is dropped.
///
/// It is guaranteed that:
///
///   - `callback` will never be called once `drop` has started.
///   - `drop` will only be called **once**, and **after** every `callback` has ended.
///   - The two previous guarantees imply that `call` and `drop` are never called concurrently.
pub struct CallbackDrop<Callback, DropFn>
where
    DropFn: FnMut() + Send + Sync + 'static,
{
    pub callback: Callback,
    pub drop: DropFn,
}

impl<Callback, DropFn> fmt::Debug for CallbackDrop<Callback, DropFn>
where
    DropFn: FnMut() + Send + Sync + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallbackDrop")
            .field("callback", &"..")
            .field("drop", &"..")
            .finish()
    }
}

impl<Callback, DropFn> Drop for CallbackDrop<Callback, DropFn>
where
    DropFn: FnMut() + Send + Sync + 'static,
{
    fn drop(&mut self) {
        (self.drop)()
    }
}

impl<OnEvent, Event, DropFn> IntoHandler<Event> for CallbackDrop<OnEvent, DropFn>
where
    OnEvent: Fn(Event) + Send + Sync + 'static,
    DropFn: FnMut() + Send + Sync + 'static,
{
    type Handler = ();

    fn into_handler(self) -> (Callback<Event>, Self::Handler) {
        (move |evt| (self.callback)(evt), ()).into_handler()
    }
}
