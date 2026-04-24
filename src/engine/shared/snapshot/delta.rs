use crate::CSnapshot;
use crate::CSnapshotBuffer;
use crate::SNAPSHOT_MAX_ITEMS;
use std::mem;
use std::pin::Pin;

use super::builder::CSnapshotBuilder;

const SNAP_MAX_TYPE: i32 = 0x7fff;
const SNAP_MAX_ID: i32 = 0xffff;
const MAX_NETOBJSIZES: usize = 64;

const TABLE_CAP: usize = (2 * SNAPSHOT_MAX_ITEMS).next_power_of_two();

#[allow(unused_must_use)] // in generated code for CSnapshotDelta_New
#[cxx::bridge]
mod ffi {
    extern "C++" {
        include!("engine/shared/snapshot.h");

        type CSnapshot = super::CSnapshot;
        type CSnapshotBuffer = super::CSnapshotBuffer;
    }
    extern "Rust" {
        type CSnapshotDelta;

        // TODO(cxx 1.0.164): #[Self = "CSnapshotDelta"]
        pub fn CSnapshotDelta_DiffItem(past: &[i32], current: &[i32], out: &mut [i32]);
        /// Create a new snapshot delta.
        ///
        /// # Example
        ///
        /// ```
        /// # extern crate ddnet_test;
        /// use ddnet_engine_shared::CSnapshotDelta_New;
        ///
        /// let delta = CSnapshotDelta_New();
        /// ```
        // TODO(cxx 1.0.164): #[Self = "CSnapshotDelta"]
        pub fn CSnapshotDelta_New() -> Box<CSnapshotDelta>;
        pub fn Clone(&mut self) -> Box<CSnapshotDelta>;
        pub fn GetDataRate(&self, type_: i32) -> u64;
        pub fn GetDataUpdates(&self, type_: i32) -> u64;
        pub fn SetStaticsize(&mut self, type_: i32, size: usize);
        pub fn EmptyDelta(&self) -> &[i32];
        pub fn CreateDelta(&mut self, from: &CSnapshot, to: &CSnapshot, delta: &mut [i32]) -> i32;
        pub fn UnpackDelta(
            &mut self,
            from: &CSnapshot,
            to: Pin<&mut CSnapshotBuffer>,
            delta: &[i32],
        ) -> i32;
    }
}

#[derive(Clone)]
struct KeyIndexTable {
    keys: [i32; TABLE_CAP],
    vals: [u16; TABLE_CAP],
    gens: [u16; TABLE_CAP],
    gen: u16,
}

impl Default for KeyIndexTable {
    fn default() -> Self {
        Self {
            keys: [0; TABLE_CAP],
            vals: [0; TABLE_CAP],
            gens: [0; TABLE_CAP],
            gen: 1,
        }
    }
}

impl KeyIndexTable {
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
        ((k.wrapping_mul(2654435761)) as usize) & (TABLE_CAP - 1)
    }

    #[inline]
    fn get(&self, key: i32) -> Option<u16> {
        let mut pos = Self::hash(key);
        loop {
            if self.gens[pos] != self.gen {
                return None;
            }
            if self.keys[pos] == key {
                return Some(self.vals[pos]);
            }
            pos = (pos + 1) & (TABLE_CAP - 1);
        }
    }

    #[inline]
    fn contains(&self, key: i32) -> bool {
        self.get(key).is_some()
    }

    #[inline]
    fn insert(&mut self, key: i32, val: u16) {
        let mut pos = Self::hash(key);
        loop {
            if self.gens[pos] != self.gen {
                self.gens[pos] = self.gen;
                self.keys[pos] = key;
                self.vals[pos] = val;
                return;
            }
            if self.keys[pos] == key {
                self.vals[pos] = val;
                return;
            }
            pos = (pos + 1) & (TABLE_CAP - 1);
        }
    }
}

#[derive(Clone)]
struct SnapView<'a> {
    buf: &'a [i32],
    num_items: usize,
    data_size_bytes: usize,
    offsets: &'a [i32],
    data_start_i32: usize,
}

impl<'a> SnapView<'a> {
    fn new(buf: &'a [i32]) -> Self {
        assert!(buf.len() >= 2);
        let data_size_bytes: usize = buf[0].try_into().expect("data size must be non-negative");
        let num_items: usize = buf[1].try_into().expect("num items must be non-negative");
        assert!(num_items <= SNAPSHOT_MAX_ITEMS);
        let data_start_i32 = 2 + num_items;
        assert!(data_start_i32 <= buf.len());
        let expected_total_bytes =
            2 * size_of::<i32>() + num_items * size_of::<i32>() + data_size_bytes;
        assert_eq!(expected_total_bytes / size_of::<i32>(), buf.len());
        let offsets = &buf[2..2 + num_items];
        Self {
            buf,
            num_items,
            data_size_bytes,
            offsets,
            data_start_i32,
        }
    }

    #[inline]
    fn item_key(&self, index: usize) -> i32 {
        let off_bytes: usize = self.offsets[index]
            .try_into()
            .expect("offset must be non-negative");
        let off_i32 = off_bytes / mem::size_of::<i32>();
        self.buf[self.data_start_i32 + off_i32]
    }

    #[inline]
    fn item_internal_type(&self, index: usize) -> u16 {
        ((self.item_key(index) as u32) >> 16) as u16
    }

    #[inline]
    fn item_id(&self, index: usize) -> u16 {
        (self.item_key(index) as u32 & 0xffff) as u16
    }

    #[inline]
    fn item_data_i32(&self, index: usize) -> &'a [i32] {
        let off_bytes: usize = self.offsets[index]
            .try_into()
            .expect("offset must be non-negative");
        let start_i32 = self.data_start_i32 + off_bytes / mem::size_of::<i32>();
        let end_off_bytes: usize = if index + 1 < self.num_items {
            self.offsets[index + 1]
                .try_into()
                .expect("offset must be non-negative")
        } else {
            self.data_size_bytes
        };
        let end_i32 = self.data_start_i32 + end_off_bytes / mem::size_of::<i32>();
        &self.buf[start_i32 + 1..end_i32]
    }
}

/// Diffs two snapshot items of the same size using the Teeworlds snapshot delta algorithm.
pub fn CSnapshotDelta_DiffItem(past: &[i32], current: &[i32], out: &mut [i32]) {
    assert_eq!(past.len(), current.len());
    assert_eq!(past.len(), out.len());
    let mut needed: u32 = 0;
    for i in 0..past.len() {
        let d = (current[i] as u32).wrapping_sub(past[i] as u32);
        out[i] = d as i32;
        needed |= d;
    }
    let _ = needed;
}

#[derive(Clone)]
struct Buffer {
    table: KeyIndexTable,
    from_index: KeyIndexTable,
    past_indices: [i16; SNAPSHOT_MAX_ITEMS],
    from_keys: [i32; SNAPSHOT_MAX_ITEMS],
    to_keys: [i32; SNAPSHOT_MAX_ITEMS],
    builder_index: KeyIndexTable,
    builder: CSnapshotBuilder,
}

impl Default for Buffer {
    fn default() -> Self {
        Self {
            table: KeyIndexTable::default(),
            from_index: KeyIndexTable::default(),
            past_indices: [-1; SNAPSHOT_MAX_ITEMS],
            from_keys: [0; SNAPSHOT_MAX_ITEMS],
            to_keys: [0; SNAPSHOT_MAX_ITEMS],
            builder_index: KeyIndexTable::default(),
            builder: CSnapshotBuilder::default(),
        }
    }
}

/// An object allowing you to operate on snapshot deltas.
#[derive(Clone)]
pub struct CSnapshotDelta {
    item_sizes: [u16; MAX_NETOBJSIZES],
    buf: Buffer,
}

impl Default for CSnapshotDelta {
    fn default() -> Self {
        Self {
            item_sizes: [0u16; MAX_NETOBJSIZES],
            buf: Buffer::default(),
        }
    }
}

/// Creates a new [`CSnapshotDelta`].
///
/// It still needs to be fed with static item sizes
/// ([`CSnapshotDelta::SetStaticsize`]).
///
/// # Example
///
/// ```
/// # extern crate ddnet_test;
/// use ddnet_engine_shared::CSnapshotDelta_New;
///
/// let delta = CSnapshotDelta_New();
/// ```
pub fn CSnapshotDelta_New() -> Box<CSnapshotDelta> {
    Box::new(CSnapshotDelta::default())
}

impl CSnapshotDelta {
    /// Creates a new [`CSnapshotDelta`] from an existing one.
    pub fn Clone(&self) -> Box<CSnapshotDelta> {
        // Avoid cloning scratch buffers.
        Box::new(CSnapshotDelta {
            item_sizes: self.item_sizes,
            buf: Buffer::default(),
        })
    }

    #[allow(missing_docs)] // not implemented
    pub fn GetDataRate(&self, type_: i32) -> u64 {
        let _ = type_;
        0
    }

    #[allow(missing_docs)] // not implemented
    pub fn GetDataUpdates(&self, type_: i32) -> u64 {
        let _ = type_;
        0
    }

    /// Tells the snapshot delta algorithm that it can assume a specific item
    /// size for a type.
    ///
    /// This function must be called with increasing `type_`, starting with 0.
    /// The `size` must be the size in bytes of the item, **not** a count of
    /// `i32`s. Pass `0` to indicate that a given item type doesn't have a
    /// known static size.
    ///
    /// Both sender and receiver must register the same static item sizes for
    /// the created snapshot deltas to be intelligible to each other.
    ///
    /// # Panics
    ///
    /// If the types aren't registered in order (see above) or if the item size
    /// is not divisible by 4.
    ///
    /// # Example
    ///
    /// ```
    /// # extern crate ddnet_test;
    /// use ddnet_engine_shared::CSnapshotDelta_New;
    ///
    /// let mut delta = CSnapshotDelta_New();
    /// delta.SetStaticsize(0, 0); // no known size
    /// delta.SetStaticsize(1, 40); // NETOBJTYPE_PLAYERINPUT, a snapshot object, for whatever reason
    /// ```
    pub fn SetStaticsize(&mut self, type_: i32, size: usize) {
        let idx: usize = type_.try_into().unwrap();
        if idx >= MAX_NETOBJSIZES {
            return;
        }
        assert!(size % 4 == 0);
        self.item_sizes[idx] = u16::try_from(size).unwrap_or(u16::MAX);
    }

    /// Returns the representation of an empty delta.
    ///
    /// I.e. the delta between a snapshot and itself.
    ///
    /// # Example
    ///
    /// ```
    /// # extern crate ddnet_test;
    /// use ddnet_engine_shared::CSnapshotDelta_New;
    ///
    /// let delta = CSnapshotDelta_New();
    /// assert_eq!(delta.EmptyDelta(), &[0, 0, 0]);
    /// ```
    pub fn EmptyDelta(&self) -> &[i32] {
        &[0; 3]
    }

    /// Diffs two snapshots to create a delta.
    ///
    /// Returns the number of bytes written to `delta`, or `-1` on error, or `0` if empty.
    pub fn CreateDelta(&mut self, from: &CSnapshot, to: &CSnapshot, delta: &mut [i32]) -> i32 {
        let from = SnapView::new(from.AsSlice());
        let to = SnapView::new(to.AsSlice());

        if delta.len() < 3 {
            return -1;
        }

        for i in 0..from.num_items {
            self.buf.from_keys[i] = from.item_key(i);
        }
        for i in 0..to.num_items {
            self.buf.to_keys[i] = to.item_key(i);
        }

        self.buf.table.clear();
        for i in 0..to.num_items {
            self.buf.table.insert(self.buf.to_keys[i], i as u16);
        }

        let mut out_pos = 3usize;
        let mut num_deleted = 0i32;
        let mut num_updated = 0i32;

        // Deleted keys.
        for i in 0..from.num_items {
            let key = self.buf.from_keys[i];
            if !self.buf.table.contains(key) {
                if out_pos >= delta.len() {
                    return -1;
                }
                delta[out_pos] = key;
                out_pos += 1;
                num_deleted += 1;
            }
        }

        // Past indices for each item in `to`.
        self.buf.table.clear();
        for i in 0..from.num_items {
            self.buf.table.insert(self.buf.from_keys[i], i as u16);
        }
        for i in 0..to.num_items {
            self.buf.past_indices[i] = match self.buf.table.get(self.buf.to_keys[i]) {
                Some(idx) => idx as i16,
                None => -1,
            };
        }

        // Updates.
        for i in 0..to.num_items {
            let internal_type = to.item_internal_type(i) as i32;
            let id = to.item_id(i) as i32;
            let cur_data = to.item_data_i32(i);
            let words = cur_data.len();
            let include_size = internal_type as usize >= MAX_NETOBJSIZES
                || self.item_sizes[internal_type as usize] == 0;

            if out_pos + (if include_size { 3 } else { 2 }) + words > delta.len() {
                return -1;
            }

            let past_idx = self.buf.past_indices[i];
            if past_idx >= 0 {
                let past_data = from.item_data_i32(past_idx as usize);
                debug_assert_eq!(past_data.len(), cur_data.len());

                let start = out_pos;
                out_pos += if include_size { 3 } else { 2 };
                let diff_pos = out_pos;

                let mut needed: u32 = 0;
                for w in 0..words {
                    let d = (cur_data[w] as u32).wrapping_sub(past_data[w] as u32);
                    delta[diff_pos + w] = d as i32;
                    needed |= d;
                }

                if needed != 0 {
                    delta[start] = internal_type;
                    delta[start + 1] = id;
                    if include_size {
                        delta[start + 2] = words as i32;
                    }
                    out_pos = diff_pos + words;
                    num_updated += 1;
                } else {
                    out_pos = start; // rollback
                }
            } else {
                delta[out_pos] = internal_type;
                delta[out_pos + 1] = id;
                if include_size {
                    delta[out_pos + 2] = words as i32;
                    out_pos += 3;
                } else {
                    out_pos += 2;
                }
                delta[out_pos..out_pos + words].copy_from_slice(cur_data);
                out_pos += words;
                num_updated += 1;
            }
        }

        if num_deleted == 0 && num_updated == 0 {
            return 0;
        }

        delta[0] = num_deleted;
        delta[1] = num_updated;
        delta[2] = 0;

        (out_pos * mem::size_of::<i32>()) as i32
    }

    /// Applies `delta` to `from` and writes the reconstructed snapshot into `to`.
    ///
    /// Returns the number of bytes written to `to`, or a negative error code.
    pub fn UnpackDelta(
        &mut self,
        from: &CSnapshot,
        to: Pin<&mut CSnapshotBuffer>,
        delta: &[i32],
    ) -> i32 {
        if delta.len() < 3 {
            return -505;
        }

        let num_deleted: usize = match delta[0].try_into() {
            Ok(v) => v,
            Err(_) => return -201,
        };
        let num_updated: usize = match delta[1].try_into() {
            Ok(v) => v,
            Err(_) => return -201,
        };
        // delta[2] is reserved/unused.

        if delta.len() < 3 + num_deleted {
            return -101;
        }

        let deleted = &delta[3..3 + num_deleted];
        let mut p = 3 + num_deleted;

        let from_view = SnapView::new(from.AsSlice());

        // Build a deleted-key set.
        self.buf.table.clear();
        for &k in deleted {
            self.buf.table.insert(k, 1);
        }

        // Build a key->index table for the base snapshot.
        self.buf.from_index.clear();
        for i in 0..from_view.num_items {
            self.buf.from_index.insert(from_view.item_key(i), i as u16);
        }

        // Start building the new snapshot in our scratch builder.
        self.buf.builder.Init(false);
        self.buf.builder_index.clear();

        // Copy everything from base snapshot that isn't deleted.
        for i in 0..from_view.num_items {
            let key = from_view.item_key(i);
            if self.buf.table.contains(key) {
                continue;
            }
            let internal_type = ((key as u32) >> 16) as i32;
            let id = (key as u32 & 0xffff) as u16;
            let data = from_view.item_data_i32(i);
            let Some((item_index, dst)) =
                self.buf
                    .builder
                    .new_item_raw_internal_with_index(internal_type, id, data.len())
            else {
                return -301;
            };
            self.buf.builder_index.insert(key, item_index as u16);
            dst.copy_from_slice(data);
        }

        // Apply updates.
        for _ in 0..num_updated {
            if p + 2 > delta.len() {
                return -102;
            }
            let type_ = delta[p];
            let id = delta[p + 1];
            p += 2;

            if type_ < 0 || type_ > SNAP_MAX_TYPE {
                return -202;
            }
            if id < 0 || id > SNAP_MAX_ID {
                return -203;
            }

            let words = if (0..(MAX_NETOBJSIZES as i32)).contains(&type_)
                && self.item_sizes[type_ as usize] != 0
            {
                (self.item_sizes[type_ as usize] as usize) / mem::size_of::<i32>()
            } else {
                if p >= delta.len() {
                    return -103;
                }
                let w = delta[p];
                p += 1;
                if w < 0 {
                    return -204;
                }
                w as usize
            };

            if p + words > delta.len() {
                return -205;
            }

            let key = (type_ << 16) | id;
            let builder_item_index = self.buf.builder_index.get(key).map(|v| v as usize);

            let dst = if let Some(idx) = builder_item_index {
                self.buf.builder.item_data_mut(idx)
            } else {
                let Some((item_index, dst)) = self
                    .buf
                    .builder
                    .new_item_raw_internal_with_index(type_, id as u16, words)
                else {
                    return -302;
                };
                self.buf.builder_index.insert(key, item_index as u16);
                dst
            };

            // Update from base snapshot if present, otherwise copy raw.
            if let Some(from_idx) = self.buf.from_index.get(key).map(|v| v as usize) {
                let past = from_view.item_data_i32(from_idx);
                debug_assert_eq!(past.len(), dst.len());
                for w in 0..words {
                    dst[w] = (past[w] as u32).wrapping_add(delta[p + w] as u32) as i32;
                }
            } else {
                dst[..words].copy_from_slice(&delta[p..p + words]);
            }
            p += words;
        }

        // Finish snapshot.
        self.buf.builder.Finish(to)
    }
}
