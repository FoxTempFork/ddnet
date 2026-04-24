use crate::CUuidManager_Global;
use crate::OFFSET_UUID;
use crate::SNAPSHOT_MAX_ITEMS;
use crate::SNAPSHOT_MAX_SIZE;
use crate::{CSnapshotBuffer, CUuid};
use std::mem;
use std::pin::Pin;

const SNAP_MAX_TYPE: i32 = 0x7fff;
const MAX_EXTENDED_ITEM_TYPES: usize = 64;

const KEYSET_CAP: usize = (2 * SNAPSHOT_MAX_ITEMS).next_power_of_two();

#[derive(Clone)]
struct KeySet {
    keys: [i32; KEYSET_CAP],
    gens: [u16; KEYSET_CAP],
    gen: u16,
}

impl Default for KeySet {
    fn default() -> Self {
        Self {
            keys: [0; KEYSET_CAP],
            gens: [0; KEYSET_CAP],
            gen: 1,
        }
    }
}

impl KeySet {
    #[inline]
    fn clear(&mut self) {
        self.gen = self.gen.wrapping_add(1);
        if self.gen == 0 {
            self.gens.fill(0);
            self.gen = 1;
        }
    }

    #[inline]
    fn hash(key: i32) -> usize {
        let k = key as u32;
        ((k.wrapping_mul(2654435761)) as usize) & (KEYSET_CAP - 1)
    }

    #[inline]
    fn contains(&self, key: i32) -> bool {
        let mut pos = Self::hash(key);
        loop {
            if self.gens[pos] != self.gen {
                return false;
            }
            if self.keys[pos] == key {
                return true;
            }
            pos = (pos + 1) & (KEYSET_CAP - 1);
        }
    }

    #[inline]
    fn insert(&mut self, key: i32) {
        let mut pos = Self::hash(key);
        loop {
            if self.gens[pos] != self.gen {
                self.gens[pos] = self.gen;
                self.keys[pos] = key;
                return;
            }
            if self.keys[pos] == key {
                return;
            }
            pos = (pos + 1) & (KEYSET_CAP - 1);
        }
    }
}

// `cxx::bridge` generates glue that uses `Box::from_raw` in a way that triggers
// `unused_must_use` warnings at the declaration site.
#[allow(unused_must_use)]
#[cxx::bridge]
mod ffi {
    extern "C++" {
        include!("engine/shared/snapshot.h");

        type CSnapshotBuffer = super::CSnapshotBuffer;
    }
    extern "Rust" {
        type CSnapshotBuilder;

        // TODO(cxx 1.0.164): #[Self = "CSnapshotBuilder"]
        pub fn CSnapshotBuilder_New() -> Box<CSnapshotBuilder>;
        pub fn Init(&mut self, sixup: bool);
        pub fn NewItem(&mut self, type_: i32, id: i32, data: &[i32]) -> bool;
        pub fn FinishIfNoDroppedItems(&mut self, buffer: Pin<&mut CSnapshotBuffer>) -> i32;
        pub fn Finish(&mut self, buffer: Pin<&mut CSnapshotBuffer>) -> i32;
    }
}

#[derive(Clone)]
struct ExtendedTypes {
    types: [i32; MAX_EXTENDED_ITEM_TYPES],
    uuid_words: [[i32; 4]; MAX_EXTENDED_ITEM_TYPES],
    len: usize,
}

impl Default for ExtendedTypes {
    fn default() -> Self {
        Self {
            types: [0; MAX_EXTENDED_ITEM_TYPES],
            uuid_words: [[0; 4]; MAX_EXTENDED_ITEM_TYPES],
            len: 0,
        }
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
#[derive(Clone)]
pub struct CSnapshotBuilder {
    building: bool,
    sixup: bool,
    has_dropped_item: bool,

    keyset: KeySet,

    // Snapshot data section (sequence of `CSnapshotItem` = `type_and_id` + data).
    data: Box<[i32; SNAPSHOT_MAX_SIZE / size_of::<i32>()]>,
    data_len_i32: usize,

    // Offsets into `data` in bytes.
    offsets: [i32; SNAPSHOT_MAX_ITEMS],
    num_items: usize,

    ext: ExtendedTypes,
}

impl Default for CSnapshotBuilder {
    fn default() -> Self {
        Self {
            building: false,
            sixup: false,
            has_dropped_item: false,
            keyset: KeySet::default(),
            data: Box::new([0; SNAPSHOT_MAX_SIZE / mem::size_of::<i32>()]),
            data_len_i32: 0,
            offsets: [0; SNAPSHOT_MAX_ITEMS],
            num_items: 0,
            ext: ExtendedTypes::default(),
        }
    }
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
        self.building = true;
        self.sixup = sixup;
        self.has_dropped_item = false;
        self.keyset.clear();
        self.data_len_i32 = 0;
        self.num_items = 0;

        // Re-add all known UUID mapping items at the start of each snapshot.
        for idx in 0..self.ext.len {
            let internal = SNAP_MAX_TYPE - (idx as i32);
            let key = ((0i32) << 16) | (internal as u16 as i32);
            let ok = self.add_extended_item_type(idx);
            // If we can't re-add existing UUID mapping items, something is very wrong.
            debug_assert!(ok);
            self.keyset.insert(key);
        }
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
        if self.has_dropped_item {
            return false;
        }

        let extended = type_ >= OFFSET_UUID;
        let internal_type = if extended {
            match self.get_extended_item_type_index(type_) {
                Some(idx) => SNAP_MAX_TYPE - (idx as i32),
                None => {
                    self.has_dropped_item = true;
                    return false;
                }
            }
        } else {
            match self.translate_type(type_) {
                Some(t) => t,
                None => return true, // dropping items from 0.7 snaps doesn't count as dropping
            }
        };

        let mut id_try: u16 = id.try_into().expect("id must fit into a 16-bit integer");
        for _ in 0..128 {
            let key = (internal_type << 16) | (id_try as i32);
            if self.keyset.contains(key) {
                // Work around #12070, try again with higher ID.
                id_try = id_try.wrapping_add(1);
                continue;
            }
            let Some(dst) = self.new_item_raw_internal(internal_type, id_try, data.len()) else {
                self.has_dropped_item = true;
                return false;
            };
            dst.copy_from_slice(data);
            self.keyset.insert(key);
            return true;
        }
        // silently drop the item if there's no ID space
        true
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

        let out = buffer.AsMutSlice();
        let total_i32 = 2 + self.num_items + self.data_len_i32;
        if total_i32 > out.len() {
            return -1;
        }
        out[0] = (self.data_len_i32 * mem::size_of::<i32>()) as i32;
        out[1] = self.num_items as i32;
        out[2..2 + self.num_items].copy_from_slice(&self.offsets[..self.num_items]);
        out[2 + self.num_items..total_i32].copy_from_slice(&self.data[..self.data_len_i32]);
        (total_i32 * mem::size_of::<i32>()) as i32
    }

    fn translate_type(&self, mut type_: i32) -> Option<i32> {
        if self.sixup {
            if type_ >= 0 {
                type_ = super::ffi::Obj_SixToSeven(type_);
                if type_ < 0 {
                    return None;
                }
            } else {
                type_ = -type_;
            }
        } else {
            assert!(
                type_ >= 0,
                "negative type is only allowed for sixup snapshots"
            );
        }
        assert!(
            (0..=SNAP_MAX_TYPE).contains(&type_),
            "type out of range: {type_}"
        );
        Some(type_)
    }

    fn total_size_with_added_item_bytes(&self, item_words: usize) -> usize {
        let header = 2 * size_of::<i32>();  // Size = header(2 i32) + offsets(num_items+1) + data(existing + new item).
        let offsets = (self.num_items + 1) * mem::size_of::<i32>();
        let data = (self.data_len_i32 + 1 + item_words) * mem::size_of::<i32>();
        header + offsets + data
    }

    pub(crate) fn new_item_raw_internal(
        &mut self,
        internal_type: i32,
        id: u16,
        item_words: usize,
    ) -> Option<&mut [i32]> {
        self.new_item_raw_internal_with_index(internal_type, id, item_words)
            .map(|(_idx, slice)| slice)
    }

    pub(crate) fn new_item_raw_internal_with_index(
        &mut self,
        internal_type: i32,
        id: u16,
        item_words: usize,
    ) -> Option<(usize, &mut [i32])> {
        assert!(self.building);
        if self.num_items >= SNAPSHOT_MAX_ITEMS {
            return None;
        }
        if self.total_size_with_added_item_bytes(item_words) > SNAPSHOT_MAX_SIZE {
            return None;
        }
        let item_i32 = 1 + item_words;
        if self.data_len_i32 + item_i32 > self.data.len() {
            return None;
        }

        let item_index = self.num_items;
        let type_and_id = (internal_type << 16) | (id as i32);
        let offset_bytes = (self.data_len_i32 * mem::size_of::<i32>()) as i32;

        self.offsets[self.num_items] = offset_bytes;
        self.num_items += 1;

        let start = self.data_len_i32;
        self.data[start] = type_and_id;
        let data_start = start + 1;
        let data_end = data_start + item_words;
        self.data[data_start..data_end].fill(0);
        self.data_len_i32 += item_i32;

        Some((item_index, &mut self.data[data_start..data_end]))
    }

    pub(crate) fn item_data_mut(&mut self, index: usize) -> &mut [i32] {
        let start_bytes = self.offsets[index] as usize;
        let start_i32 = start_bytes / mem::size_of::<i32>();
        let end_bytes = if index + 1 < self.num_items {
            self.offsets[index + 1] as usize
        } else {
            self.data_len_i32 * mem::size_of::<i32>()
        };
        let end_i32 = end_bytes / mem::size_of::<i32>();
        &mut self.data[start_i32 + 1..end_i32]
    }

    fn get_extended_item_type_index(&mut self, type_id: i32) -> Option<usize> {
        for i in 0..self.ext.len {
            if self.ext.types[i] == type_id {
                return Some(i);
            }
        }
        if self.ext.len >= MAX_EXTENDED_ITEM_TYPES {
            return None;
        }
        let idx = self.ext.len;
        let uuid = CUuidManager_Global().GetUuid(type_id);
        let b = *uuid.as_bytes();
        let mut words = [0i32; 4];
        for i in 0..4 {
            let w = u32::from_be_bytes(b[i * 4..i * 4 + 4].try_into().unwrap());
            words[i] = w as i32;
        }
        self.ext.types[idx] = type_id;
        self.ext.uuid_words[idx] = words;
        self.ext.len += 1;
        if self.add_extended_item_type(idx) {
            Some(idx)
        } else {
            self.ext.len -= 1;
            None
        }
    }

    fn add_extended_item_type(&mut self, idx: usize) -> bool {
        assert!(self.building);
        let internal = SNAP_MAX_TYPE - (idx as i32);
        let uuid_item_data_words = mem::size_of::<CUuid>() / mem::size_of::<i32>();
        debug_assert_eq!(uuid_item_data_words, 4);
        let words = self.ext.uuid_words[idx];

        let Some(dst) = self.new_item_raw_internal(0, internal as u16, uuid_item_data_words) else {
            return false;
        };
        dst.copy_from_slice(&words);
        true
    }
}
