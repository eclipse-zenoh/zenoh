# del_link Duplicate Event Bug

## Location
`io/zenoh-transport/src/unicast/universal/transport.rs:159-210`

## Problem
Duplicate Delete events are sent for link disconnection due to a race condition in `del_link()`.

## Root Cause
When it's the **last link** (`is_last == true`), the lock is dropped WITHOUT removing the link first:

```rust
if is_last {
    // Close the whole transport
    drop(guard);  // <-- Lock released, but link still in list!
    Target::Transport
} else {
    // Remove the link
    let mut links = guard.to_vec();
    let stl = links.remove(index);
    *guard = links.into_boxed_slice();
    drop(guard);
    Target::Link(stl.into())
}
```

Then `callback.del_link(link)` is called AFTER the lock is released.

## Race Condition
1. Thread A (handle_close): `del_link()` → finds link, `is_last=true`, drops lock at line 181
2. Thread B (rx_task failure): `del_link()` → acquires lock, finds **same link still there**, `is_last=true`, drops lock
3. Thread A: calls `callback.del_link()` → **first Delete event**
4. Thread B: calls `callback.del_link()` → **second Delete event (DUPLICATE)**

When `is_last == false`, the link IS removed before dropping the lock, preventing this race.

## Proposed Fix
Remove the link from the list even when `is_last`, before dropping the lock:

```rust
if is_last {
    // Remove the link before dropping the lock to prevent race
    *guard = vec![].into_boxed_slice();
    drop(guard);
    Target::Transport
} else {
    // ... existing code
}
```

## Multiple Trigger Paths
`del_link()` can be triggered from multiple code paths concurrently:
1. `rx.rs:67-82` - `handle_close()` when receiving CLOSE message
2. `link.rs:176-188` - RX task failure error handler
3. `link.rs:143-151` - TX task failure error handler
