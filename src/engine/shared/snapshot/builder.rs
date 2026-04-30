use super::type_id_from_i32;
use crate::CSnapshotBuffer;
use libtw2_common::Takeable;
use libtw2_snapshot::snap;
use std::mem;
use std::pin::Pin;

#[allow(unused_must_use)] // in generated code for CSnapshotBuilder_New
#[cxx::bridge]
mod ffi {
    extern "C++" {
        include!("engine/shared/snapshot.h");

        type CSnapshotBuffer = super::CSnapshotBuffer;
    }
    extern "Rust" {
        type CSnapshotBuilder;

        // TODO(cxx 1.0.164): #[Self = "CSnapshotBuilder"]
        #[allow(unused_must_use)]
        pub fn CSnapshotBuilder_New() -> Box<CSnapshotBuilder>;
        pub fn Init(&mut self, sixup: bool);
        pub fn NewItem(&mut self, type_: i32, id: i32, data: &[i32]) -> bool;
        pub fn NewItemsFlat(
            &mut self,
            types: &[i32],
            ids: &[i32],
            data: &[i32],
            offsets: &[u32],
        ) -> bool;
        pub fn FinishIfNoDroppedItems(&mut self, buffer: Pin<&mut CSnapshotBuffer>) -> i32;
        pub fn Finish(&mut self, buffer: Pin<&mut CSnapshotBuffer>) -> i32;
    }
}

/// A (reusable) object for building snapshots.
///
/// # Example
///
/// ```
/// # extern crate ddnet_test;
/// use ddnet_engine_shared::CSnapshotBuffer_New;
/// use ddnet_engine_shared::CSnapshotBuilder_New;
///
/// let mut builder = CSnapshotBuilder_New();
/// builder.Init(false);
/// assert!(builder.NewItem(1, 123, &[1, 2, 3]));
///
/// let mut buffer = CSnapshotBuffer_New();
/// assert_eq!(builder.Finish(buffer.pin_mut()), 28);
/// assert_eq!(&buffer.pin_mut().AsMutSlice()[..7], &[
///     16, // data size in bytes
///     1, // 1 item
///     0, // offset of first item in data section: 0 bytes
///     // data starting here
///     65536 + 123, // type ID (shifted 16 to the left) plus ID
///     1,
///     2,
///     3,
/// ]);
/// ```
#[derive(Default)]
pub struct CSnapshotBuilder {
    building: bool,
    sixup: bool,
    has_dropped_item: bool,
    use_borrowed: bool,
    snap_write_buffer: Vec<i32>,
    borrowed: snap::BorrowedRawBuilder,
    builder: Takeable<snap::Builder>,
}

/// Creates a new [`CSnapshotBuilder`].
/// ```
/// # extern crate ddnet_test;
/// use ddnet_engine_shared::CSnapshotBuilder_New;
///
/// let builder = CSnapshotBuilder_New();
/// ```
pub fn CSnapshotBuilder_New() -> Box<CSnapshotBuilder> {
    Box::new(CSnapshotBuilder::default())
}

impl CSnapshotBuilder {
    /// Starts building a snapshot.
    ///
    /// `sixup` indicates whether the builder should do translation for a 0.7
    /// client.
    ///
    /// See [`CSnapshotBuilder`] for a usage example.
    ///
    /// # Panics
    ///
    /// Panics if `Init` was previously called without a call to
    /// [`CSnapshotBuilder::Finish`]/[`CSnapshotBuilder::FinishIfNoDroppedItems`].
    pub fn Init(&mut self, sixup: bool) {
        assert!(!self.building);
        *self = CSnapshotBuilder {
            building: true,
            sixup,
            has_dropped_item: false,
            use_borrowed: true,
            snap_write_buffer: mem::take(&mut self.snap_write_buffer),
            borrowed: mem::take(&mut self.borrowed),
            builder: Takeable::new(self.builder.take()),
        };
        self.borrowed.clear();
    }

    /// Adds an item to the snapshot.
    ///
    /// Items are identified by a type and an ID, and can contain some number
    /// of `i32`s. The number of `i32`s usually doesn't change for a given
    /// type.
    ///
    /// Returns whether the item was actually added to the snapshot. It might
    /// be dropped due to the size limit for snapshots
    /// ([`SNAPSHOT_MAX_SIZE`](crate::SNAPSHOT_MAX_SIZE)), or due to the
    /// maximum number of items in a snapshot
    /// ([`SNAPSHOT_MAX_ITEMS`](crate::SNAPSHOT_MAX_ITEMS)).
    ///
    /// See [`CSnapshotBuilder`] for a usage example.
    ///
    /// # Panics
    ///
    /// Panics if [`CSnapshotBuilder::Init`] wasn't called prior to this call.
    /// Panics if the `id` doesn't fit into a 16-bit unsigned integer. Panics
    /// if the `type_` doesn't fit into a 16-bit unsigned integer and isn't a
    /// known UUID item type.
    pub fn NewItem(&mut self, type_: i32, id: i32, data: &[i32]) -> bool {
        assert!(self.building);

        // Fast path for the overwhelmingly common case: non-sixup, ordinal type IDs.
        // This avoids constructing `TypeId` and going through UUID translation logic.
        if !self.sixup && 0 < type_ && type_ < crate::OFFSET_UUID {
            let raw_type_id_u32 = type_ as u32;
            if raw_type_id_u32 < (libtw2_snapshot::format::OFFSET_EXTENDED_TYPE_ID as u32) {
                let raw_type_id = raw_type_id_u32 as u16;
                let mut id_u16: u16 = id.try_into().expect("id must fit into a 16-bit integer");
                for _ in 0..128 {
                    let res = if self.use_borrowed {
                        unsafe {
                            self.borrowed.add_item_borrowed(
                                raw_type_id,
                                id_u16,
                                data.as_ptr(),
                                data.len(),
                            )
                        }
                    } else {
                        self.builder.add_item(
                            libtw2_snapshot::format::TypeId::Ordinal(raw_type_id),
                            id_u16,
                            data,
                        )
                    };
                    match res {
                        Ok(()) => return true,
                        Err(snap::BuilderError::DuplicateKey) => {
                            // Work around #12070, try again with higher ID:
                            id_u16 = id_u16.wrapping_add(1);
                            continue;
                        }
                        Err(snap::BuilderError::TooLongSnap | snap::BuilderError::TooManyItems) => {
                            self.has_dropped_item = true;
                            return false;
                        }
                    }
                }
                // silently drop the item if there's no ID space
                return true;
            }
        }

        let type_ = match type_id_from_i32(self.sixup, type_) {
            Some(t) => t,
            None => return true, // dropping items from 0.7 snaps doesn't count as dropping
        };
        let mut id: u16 = id.try_into().expect("id must fit into a 16-bit integer");
        for _ in 0..128 {
            let res = match type_ {
                libtw2_snapshot::format::TypeId::Ordinal(raw_type_id) if self.use_borrowed => unsafe {
                    self.borrowed
                        .add_item_borrowed(raw_type_id, id, data.as_ptr(), data.len())
                },
                libtw2_snapshot::format::TypeId::Uuid(_) if self.use_borrowed => {
                    // UUID types require an owned builder (it injects NETOBJTYPE_EX items).
                    // Copy already-added ordinal items into the owned builder, then continue there.
                    unsafe {
                        if self.borrowed.drain_into_builder(&mut self.builder).is_err() {
                            self.has_dropped_item = true;
                            return false;
                        }
                    }
                    self.use_borrowed = false;
                    self.builder.add_item(type_, id, data)
                }
                _ => self.builder.add_item(type_, id, data),
            };

            return match res {
                Ok(()) => true,
                Err(snap::BuilderError::DuplicateKey) => {
                    // Work around #12070, try again with higher ID:
                    id = id.wrapping_add(1);
                    continue;
                }
                Err(snap::BuilderError::TooLongSnap | snap::BuilderError::TooManyItems) => {
                    self.has_dropped_item = true;
                    false
                }
            };
        }
        // silently drop the item if there's no ID space
        true
    }

    /// Adds multiple items in one call, using a flat data buffer + offsets.
    ///
    /// `types.len()` must match `ids.len()`. `offsets.len()` must be
    /// `types.len() + 1`, monotonically non-decreasing, and end with
    /// `data.len()`.
    ///
    /// Returns whether all items were added (i.e. no item was dropped due to
    /// snapshot limits).
    pub fn NewItemsFlat(
        &mut self,
        types: &[i32],
        ids: &[i32],
        data: &[i32],
        offsets: &[u32],
    ) -> bool {
        assert!(self.building);
        if types.len() != ids.len() {
            return false;
        }
        if offsets.len() != types.len() + 1 {
            return false;
        }
        let mut prev = 0usize;
        for &off in offsets {
            let off = off as usize;
            if off < prev || off > data.len() {
                return false;
            }
            prev = off;
        }
        if prev != data.len() {
            return false;
        }

        for i in 0..types.len() {
            let start = offsets[i] as usize;
            let end = offsets[i + 1] as usize;
            // Preserve `NewItem` semantics for sixup drops and duplicate workarounds.
            self.NewItem(types[i], ids[i], &data[start..end]);
        }
        !self.has_dropped_item
    }

    /// Finishes building the snapshot, erroring if any items were dropped.
    ///
    /// See [`CSnapshotBuilder::Finish`] for more details.
    pub fn FinishIfNoDroppedItems(&mut self, buffer: Pin<&mut CSnapshotBuffer>) -> i32 {
        assert!(self.building);
        if self.has_dropped_item {
            self.building = false;
            return -1;
        }
        self.Finish(buffer)
    }

    /// Finishes building the snapshot, erroring if any items were dropped.
    ///
    /// Returns the number of bytes written to `buffer` on success, or `-1` on
    /// error.
    ///
    /// See [`CSnapshotBuilder`] for a usage example.
    ///
    /// # Panics
    ///
    /// Panics if [`CSnapshotBuilder::Init`] wasn't called prior to this call.
    pub fn Finish(&mut self, buffer: Pin<&mut CSnapshotBuffer>) -> i32 {
        assert!(self.building);
        self.building = false;
        if self.use_borrowed {
            let out = buffer.AsMutSlice();
            let result = match unsafe { self.borrowed.write_to_ints_borrowed(out) } {
                Ok(written) => std::mem::size_of_val(written).try_into().unwrap(),
                Err(_) => -1,
            };
            self.borrowed.clear();
            return result;
        }

        let snap = self.builder.take().finish();
        let result = match snap.write_to_ints(&mut self.snap_write_buffer, buffer.AsMutSlice()) {
            Ok(result) => std::mem::size_of_val(result).try_into().unwrap(),
            Err(_) => -1,
        };
        self.builder.restore(snap.recycle());
        result
    }
}
