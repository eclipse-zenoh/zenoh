use std::{io, io::IoSlice, mem};

use tokio::io::{AsyncWrite, AsyncWriteExt};

// From IoSlice::advance_slices
fn advance_slices(bufs: &mut &mut [IoSlice<'_>], n: usize) {
    let mut remove = 0;
    let mut left = n;
    for buf in bufs.iter() {
        if let Some(remainder) = left.checked_sub(buf.len()) {
            left = remainder;
            remove += 1;
        } else {
            break;
        }
    }
    *bufs = &mut mem::take(bufs)[remove..];
    if bufs.is_empty() {
        assert!(left == 0, "advancing io slices beyond their length");
    } else {
        // SAFETY IoSlice::as_slice is defined as IoSlice::deref
        bufs[0] = IoSlice::new(unsafe { &*(&bufs[0][left..] as *const [u8]) });
    }
}

pub async fn write_all_vectored(
    writer: &mut (impl AsyncWrite + Unpin),
    mut bufs: &mut [IoSlice<'_>],
) -> io::Result<()> {
    // Guarantee that bufs is empty if it contains no data,
    // to avoid calling write_vectored if there is no data to be written.
    advance_slices(&mut bufs, 0);
    while !bufs.is_empty() {
        match writer.write_vectored(bufs).await {
            Ok(0) => {
                return Err(io::ErrorKind::WriteZero.into());
            }
            Ok(n) => advance_slices(&mut bufs, n),
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
