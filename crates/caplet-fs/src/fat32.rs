//! FAT32 filesystem backend.
//!
//! This is a self-contained, `no_std` FAT32 implementation.  It parses the
//! BPB, follows FAT chains, understands both 8.3 and long-file-name directory
//! entries, and writes files, directories and renames directly into the block
//! image.  The image is kept in RAM and can be checkpointed byte-for-byte.
//!
//! The implementation is intentionally conservative: files are buffered in
//! open handles and committed on `flush`/`close`; allocation is performed for
//! the complete new cluster chain before the old chain is released.  This keeps
//! an interrupted commit recoverable by a normal FAT consistency check.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use caplet_sync::spin::SpinLock;

use crate::driver::{DirEntry, DriverError, FileMode, FsDriver, SeekFrom};

const ATTR_READ_ONLY: u8 = 0x01;
const ATTR_DIRECTORY: u8 = 0x10;
const ATTR_ARCHIVE: u8 = 0x20;
const ATTR_LFN: u8 = 0x0f;
const FAT_EOC: u32 = 0x0fff_ffff;

/// A mountable FAT32 image.
pub struct Fat32 {
    state: SpinLock<State>,
}

struct State {
    image: Vec<u8>,
    handles: BTreeMap<u32, Handle>,
    next_handle: u32,
}

struct Handle {
    path: String,
    data: Vec<u8>,
    pos: usize,
    mode: FileMode,
    dirty: bool,
}

/// Backwards-compatible alias for the former placeholder.
pub type Fat32Placeholder = Fat32;
/// Descriptive alias.
pub type Fat32Driver = Fat32;

#[derive(Debug, Clone, Copy)]
struct Geometry {
    bytes_per_sector: u32,
    reserved_sectors: u32,
    num_fats: u32,
    fat_size: u32,
    root_cluster: u32,
    fat_start: u32,
    data_start: u32,
    cluster_count: u32,
    cluster_size: u32,
}

#[derive(Debug, Clone)]
struct RawEntry {
    attr: u8,
    cluster: u32,
    size: u32,
    start: usize,
    end: usize,
}

impl Fat32 {
    /// Mount an existing FAT32 image.
    pub fn from_bytes(bytes: &[u8]) -> Result<Arc<Self>, DriverError> {
        Ok(Arc::new(Self::try_from_bytes(bytes)?))
    }

    /// Mount an existing image and return the driver by value.
    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, DriverError> {
        let geom = Geometry::parse(bytes)?;
        if geom.fat_start as usize >= bytes.len() {
            return Err(DriverError::IoError);
        }
        Ok(Self {
            state: SpinLock::new(State {
                image: bytes.to_vec(),
                handles: BTreeMap::new(),
                next_handle: 1,
            }),
        })
    }

    /// Format a new FAT32 volume.  `total_bytes` must yield at least 65525
    /// data clusters, as required by the FAT specification.
    pub fn format(total_bytes: usize, sectors_per_cluster: u32) -> Result<Self, DriverError> {
        if sectors_per_cluster == 0 || !sectors_per_cluster.is_power_of_two() {
            return Err(DriverError::InvalidArgument);
        }
        let image = format_image(total_bytes, sectors_per_cluster)?;
        let geom = Geometry::parse(&image)?;
        if geom.cluster_count < 65_525 {
            return Err(DriverError::InvalidArgument);
        }
        Ok(Self {
            state: SpinLock::new(State {
                image,
                handles: BTreeMap::new(),
                next_handle: 1,
            }),
        })
    }

    /// Export the durable image bytes.  Open handles that have not been flushed
    /// are not included.
    pub fn image_bytes(&self) -> Vec<u8> {
        self.state.lock().image.clone()
    }

    /// Restore a checkpoint, rejecting malformed images.
    pub fn try_restore_from_checkpoint(bytes: &[u8]) -> Result<Self, DriverError> {
        Self::try_from_bytes(bytes)
    }

    fn flush_locked(state: &mut State, id: u32) -> Result<(), DriverError> {
        let (path, data, dirty) = {
            let h = state.handles.get(&id).ok_or(DriverError::InvalidArgument)?;
            (h.path.clone(), h.data.clone(), h.dirty)
        };
        if !dirty {
            return Ok(());
        }
        let geom = Geometry::parse(&state.image)?;
        let (parent_path, name) = split_parent(&path)?;
        let parent = resolve_dir(&state.image, geom, &parent_path)?;
        let existing = find_entry(&state.image, geom, parent, &name)?;

        let new_chain = allocate_chain(&mut state.image, geom, data.len())?;
        if !new_chain.is_empty() {
            write_chain(&mut state.image, geom, &new_chain, &data)?;
        }
        let first = new_chain.first().copied().unwrap_or(0);
        if let Some(entry) = existing {
            if entry.attr & ATTR_DIRECTORY != 0 {
                if !new_chain.is_empty() {
                    free_chain(&mut state.image, geom, first)?;
                }
                return Err(DriverError::InvalidArgument);
            }
            free_chain(&mut state.image, geom, entry.cluster)?;
            update_entry(&mut state.image, &entry, first, data.len() as u32)?;
            make_dirty(&mut state.image, &entry)?;
        } else {
            create_entry(
                &mut state.image,
                geom,
                parent,
                &name,
                ATTR_ARCHIVE,
                first,
                data.len() as u32,
            )?;
        }
        if let Some(h) = state.handles.get_mut(&id) {
            h.dirty = false;
        }
        Ok(())
    }
}

impl FsDriver for Fat32 {
    fn open(&self, path: &str, mode: FileMode) -> Result<u32, DriverError> {
        let path = normalize(path)?;
        let mut state = self.state.lock();
        let geom = Geometry::parse(&state.image)?;
        let (parent_path, name) = split_parent(&path)?;
        let parent = resolve_dir(&state.image, geom, &parent_path)?;
        let existing = find_entry(&state.image, geom, parent, &name)?;
        if matches!(mode, FileMode::CreateNew) && existing.is_some() {
            return Err(DriverError::AlreadyExists);
        }
        if let Some(e) = &existing {
            if e.attr & ATTR_DIRECTORY != 0 {
                return Err(DriverError::InvalidArgument);
            }
            if e.attr & ATTR_READ_ONLY != 0 && mode.is_write() {
                return Err(DriverError::PermissionDenied);
            }
        }
        let create = matches!(mode, FileMode::CreateNew | FileMode::CreateTruncate);
        if existing.is_none() && !create {
            return Err(DriverError::NotFound);
        }
        let truncate = matches!(mode, FileMode::CreateTruncate);
        let data = match existing.as_ref() {
            Some(entry) if !truncate => read_file_data(&state.image, geom, entry)?,
            _ => Vec::new(),
        };
        let pos = if matches!(mode, FileMode::Append) {
            data.len()
        } else {
            0
        };
        let id = state.next_handle;
        state.next_handle = state
            .next_handle
            .checked_add(1)
            .ok_or(DriverError::NoSpace)?;
        state.handles.insert(
            id,
            Handle {
                path,
                data,
                pos,
                mode,
                dirty: truncate || existing.is_none(),
            },
        );
        Ok(id)
    }

    fn close(&self, handle: u32) -> Result<(), DriverError> {
        let mut state = self.state.lock();
        Self::flush_locked(&mut state, handle)?;
        state
            .handles
            .remove(&handle)
            .ok_or(DriverError::InvalidArgument)?;
        Ok(())
    }

    fn read(&self, handle: u32, buf: &mut [u8]) -> Result<usize, DriverError> {
        let mut state = self.state.lock();
        let h = state
            .handles
            .get_mut(&handle)
            .ok_or(DriverError::InvalidArgument)?;
        if !h.mode.is_read() {
            return Err(DriverError::PermissionDenied);
        }
        if h.pos >= h.data.len() {
            return Ok(0);
        }
        let n = buf.len().min(h.data.len() - h.pos);
        buf[..n].copy_from_slice(&h.data[h.pos..h.pos + n]);
        h.pos += n;
        Ok(n)
    }

    fn write(&self, handle: u32, buf: &[u8]) -> Result<usize, DriverError> {
        let mut state = self.state.lock();
        let h = state
            .handles
            .get_mut(&handle)
            .ok_or(DriverError::InvalidArgument)?;
        if !h.mode.is_write() {
            return Err(DriverError::PermissionDenied);
        }
        if matches!(h.mode, FileMode::Append) {
            h.pos = h.data.len();
        }
        let end = h.pos.checked_add(buf.len()).ok_or(DriverError::NoSpace)?;
        if end > h.data.len() {
            h.data.resize(end, 0);
        }
        h.data[h.pos..end].copy_from_slice(buf);
        h.pos = end;
        h.dirty = true;
        Ok(buf.len())
    }

    fn seek(&self, handle: u32, from: SeekFrom) -> Result<u64, DriverError> {
        let mut state = self.state.lock();
        let h = state
            .handles
            .get_mut(&handle)
            .ok_or(DriverError::InvalidArgument)?;
        let base = match from {
            SeekFrom::Start(v) => i128::from(v),
            SeekFrom::Current(v) => h.pos as i128 + i128::from(v),
            SeekFrom::End(v) => h.data.len() as i128 + i128::from(v),
        };
        if base < 0 || base > u64::MAX as i128 {
            return Err(DriverError::InvalidArgument);
        }
        h.pos = base as usize;
        Ok(base as u64)
    }

    fn flush(&self, handle: u32) -> Result<(), DriverError> {
        let mut state = self.state.lock();
        Self::flush_locked(&mut state, handle)
    }

    fn unlink(&self, path: &str) -> Result<(), DriverError> {
        let path = normalize(path)?;
        let mut state = self.state.lock();
        let geom = Geometry::parse(&state.image)?;
        let (parent_path, name) = split_parent(&path)?;
        let parent = resolve_dir(&state.image, geom, &parent_path)?;
        let entry = find_entry(&state.image, geom, parent, &name)?.ok_or(DriverError::NotFound)?;
        if entry.attr & ATTR_DIRECTORY != 0 {
            return Err(DriverError::InvalidArgument);
        }
        free_chain(&mut state.image, geom, entry.cluster)?;
        mark_deleted(&mut state.image, entry.start, entry.end);
        Ok(())
    }

    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, DriverError> {
        let path = normalize(path)?;
        let state = self.state.lock();
        let geom = Geometry::parse(&state.image)?;
        let cluster = resolve_dir(&state.image, geom, &path)?;
        entries_in_dir(&state.image, geom, cluster)
    }

    fn mkdir(&self, path: &str) -> Result<(), DriverError> {
        let path = normalize(path)?;
        let mut state = self.state.lock();
        let geom = Geometry::parse(&state.image)?;
        let (parent_path, name) = split_parent(&path)?;
        let parent = resolve_dir(&state.image, geom, &parent_path)?;
        if find_entry(&state.image, geom, parent, &name)?.is_some() {
            return Err(DriverError::AlreadyExists);
        }
        let cluster = alloc_cluster(&mut state.image, geom)?;
        let off = cluster_offset(geom, cluster)?;
        initialize_directory(&mut state.image, geom, off, cluster, parent)?;
        create_entry(
            &mut state.image,
            geom,
            parent,
            &name,
            ATTR_DIRECTORY,
            cluster,
            0,
        )?;
        Ok(())
    }

    fn rmdir(&self, path: &str) -> Result<(), DriverError> {
        let path = normalize(path)?;
        if path == "/" {
            return Err(DriverError::InvalidArgument);
        }
        let mut state = self.state.lock();
        let geom = Geometry::parse(&state.image)?;
        let (parent_path, name) = split_parent(&path)?;
        let parent = resolve_dir(&state.image, geom, &parent_path)?;
        let entry = find_entry(&state.image, geom, parent, &name)?.ok_or(DriverError::NotFound)?;
        if entry.attr & ATTR_DIRECTORY == 0 {
            return Err(DriverError::InvalidArgument);
        }
        if !entries_in_dir(&state.image, geom, entry.cluster)?.is_empty() {
            return Err(DriverError::InvalidArgument);
        }
        free_chain(&mut state.image, geom, entry.cluster)?;
        mark_deleted(&mut state.image, entry.start, entry.end);
        Ok(())
    }

    fn rename(&self, src: &str, dst: &str) -> Result<(), DriverError> {
        let src = normalize(src)?;
        let dst = normalize(dst)?;
        if src == "/" || dst == "/" {
            return Err(DriverError::InvalidArgument);
        }
        let mut state = self.state.lock();
        let geom = Geometry::parse(&state.image)?;
        let (src_parent_path, src_name) = split_parent(&src)?;
        let (dst_parent_path, dst_name) = split_parent(&dst)?;
        let src_parent = resolve_dir(&state.image, geom, &src_parent_path)?;
        let dst_parent = resolve_dir(&state.image, geom, &dst_parent_path)?;
        let entry =
            find_entry(&state.image, geom, src_parent, &src_name)?.ok_or(DriverError::NotFound)?;
        if find_entry(&state.image, geom, dst_parent, &dst_name)?.is_some() {
            return Err(DriverError::AlreadyExists);
        }
        let created = create_entry(
            &mut state.image,
            geom,
            dst_parent,
            &dst_name,
            entry.attr,
            entry.cluster,
            entry.size,
        )?;
        if entry.attr & ATTR_DIRECTORY != 0 {
            let off = cluster_offset(geom, entry.cluster)?;
            let parent_slot = find_dotdot_offset(&state.image, geom, entry.cluster)?;
            if let Some(slot) = parent_slot {
                write_u32(&mut state.image, slot + 26, dst_parent)?;
            } else if off < state.image.len() {
                // A directory created outside this implementation may have no
                // `..`; nothing else is required for the move.
            }
        }
        mark_deleted(&mut state.image, entry.start, entry.end);
        let _ = created;
        for h in state.handles.values_mut() {
            if h.path == src {
                h.path = dst.clone();
            } else if h.path.starts_with(&src) && h.path.as_bytes().get(src.len()) == Some(&b'/') {
                h.path = dst.clone() + &h.path[src.len()..];
            }
        }
        Ok(())
    }

    fn statvfs(&self) -> Result<(u64, u64), DriverError> {
        let state = self.state.lock();
        let geom = Geometry::parse(&state.image)?;
        let free_clusters = free_cluster_count(&state.image, geom)? as u64;
        let total = u64::from(geom.cluster_count) * u64::from(geom.cluster_size);
        let free = free_clusters * u64::from(geom.cluster_size);
        Ok((total, free))
    }

    fn sync(&self) -> Result<(), DriverError> {
        let ids: Vec<u32> = self.state.lock().handles.keys().copied().collect();
        for id in ids {
            self.flush(id)?;
        }
        Ok(())
    }
}

impl Geometry {
    fn parse(image: &[u8]) -> Result<Self, DriverError> {
        if image.len() < 512 || image[510] != 0x55 || image[511] != 0xaa {
            return Err(DriverError::IoError);
        }
        let bytes_per_sector = u16::from_le_bytes([image[11], image[12]]) as u32;
        let sectors_per_cluster = image[13] as u32;
        let reserved_sectors = u16::from_le_bytes([image[14], image[15]]) as u32;
        let num_fats = image[16] as u32;
        let total16 = u16::from_le_bytes([image[19], image[20]]) as u32;
        let total32 = u32::from_le_bytes([image[32], image[33], image[34], image[35]]);
        let fat16 = u16::from_le_bytes([image[22], image[23]]) as u32;
        let fat32 = u32::from_le_bytes([image[36], image[37], image[38], image[39]]);
        let root_cluster = u32::from_le_bytes([image[44], image[45], image[46], image[47]]);
        let total_sectors = if total16 != 0 { total16 } else { total32 };
        let fat_size = if fat16 != 0 { fat16 } else { fat32 };
        if bytes_per_sector < 512
            || !bytes_per_sector.is_power_of_two()
            || sectors_per_cluster == 0
            || !sectors_per_cluster.is_power_of_two()
            || reserved_sectors == 0
            || num_fats == 0
            || fat_size == 0
            || total_sectors <= reserved_sectors + num_fats * fat_size
            || root_cluster < 2
        {
            return Err(DriverError::IoError);
        }
        let data_start = reserved_sectors + num_fats * fat_size;
        let cluster_count = total_sectors - data_start;
        if cluster_count < 1 || total_sectors as usize * bytes_per_sector as usize > image.len() {
            return Err(DriverError::IoError);
        }
        Ok(Self {
            bytes_per_sector,
            reserved_sectors,
            num_fats,
            fat_size,
            root_cluster,
            fat_start: reserved_sectors * bytes_per_sector,
            data_start: data_start * bytes_per_sector,
            cluster_count,
            cluster_size: sectors_per_cluster * bytes_per_sector,
        })
    }
}

fn split_parent(path: &str) -> Result<(String, String), DriverError> {
    if path == "/" {
        return Err(DriverError::InvalidArgument);
    }
    let (parent, name) = path.rsplit_once('/').ok_or(DriverError::InvalidName)?;
    if name.is_empty() || name.len() > 255 || name.contains('/') {
        return Err(DriverError::InvalidName);
    }
    let parent = if parent.is_empty() {
        String::from("/")
    } else {
        parent.to_string()
    };
    Ok((parent, name.to_string()))
}

fn normalize(path: &str) -> Result<String, DriverError> {
    if path.is_empty() {
        return Ok(String::from("/"));
    }
    if !path.starts_with('/') {
        return Err(DriverError::InvalidName);
    }
    let mut out = String::new();
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(DriverError::InvalidName);
        }
        out.push('/');
        out.push_str(segment);
    }
    if out.is_empty() {
        out.push('/');
    }
    Ok(out)
}

fn cluster_offset(geom: Geometry, cluster: u32) -> Result<usize, DriverError> {
    if cluster < 2 || cluster >= geom.cluster_count + 2 {
        return Err(DriverError::IoError);
    }
    Ok(geom.data_start as usize + (cluster as usize - 2) * geom.cluster_size as usize)
}

fn read_u32(image: &[u8], off: usize) -> Result<u32, DriverError> {
    image
        .get(off..off + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(DriverError::IoError)
}

fn write_u32(image: &mut [u8], off: usize, value: u32) -> Result<(), DriverError> {
    let dst = image.get_mut(off..off + 4).ok_or(DriverError::IoError)?;
    dst.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn fat_entry(image: &[u8], geom: Geometry, cluster: u32) -> Result<u32, DriverError> {
    let off = geom.fat_start as usize + cluster as usize * 4;
    Ok(read_u32(image, off)? & 0x0fff_ffff)
}

fn set_fat_entry(
    image: &mut [u8],
    geom: Geometry,
    cluster: u32,
    value: u32,
) -> Result<(), DriverError> {
    for copy in 0..geom.num_fats {
        let off = geom.fat_start as usize
            + copy as usize * geom.fat_size as usize * geom.bytes_per_sector as usize
            + cluster as usize * 4;
        let old = read_u32(image, off)?;
        write_u32(image, off, (old & 0xf000_0000) | (value & 0x0fff_ffff))?;
    }
    Ok(())
}

fn cluster_chain(image: &[u8], geom: Geometry, first: u32) -> Result<Vec<u32>, DriverError> {
    let mut out = Vec::new();
    if first == 0 {
        return Ok(out);
    }
    let mut cur = first;
    let limit = geom.cluster_count + 2;
    while cur >= 2 && cur < limit {
        if out.len() > geom.cluster_count as usize {
            return Err(DriverError::IoError);
        }
        out.push(cur);
        let next = fat_entry(image, geom, cur)?;
        if next >= FAT_EOC || next == 0 || next == 1 {
            break;
        }
        cur = next;
    }
    Ok(out)
}

fn allocate_chain(image: &mut [u8], geom: Geometry, len: usize) -> Result<Vec<u32>, DriverError> {
    let needed = len.div_ceil(geom.cluster_size as usize);
    let mut chain = Vec::with_capacity(needed);
    for _ in 0..needed {
        let c = alloc_cluster(image, geom)?;
        chain.push(c);
    }
    for pair in chain.windows(2) {
        set_fat_entry(image, geom, pair[0], pair[1])?;
    }
    if let Some(last) = chain.last() {
        set_fat_entry(image, geom, *last, FAT_EOC)?;
    }
    Ok(chain)
}

fn alloc_cluster(image: &mut [u8], geom: Geometry) -> Result<u32, DriverError> {
    for cluster in 2..geom.cluster_count + 2 {
        if fat_entry(image, geom, cluster)? == 0 {
            let off = cluster_offset(geom, cluster)?;
            let end = off + geom.cluster_size as usize;
            image.get_mut(off..end).ok_or(DriverError::IoError)?.fill(0);
            set_fat_entry(image, geom, cluster, FAT_EOC)?;
            refresh_fsinfo(image, geom)?;
            return Ok(cluster);
        }
    }
    Err(DriverError::NoSpace)
}

fn free_chain(image: &mut [u8], geom: Geometry, first: u32) -> Result<(), DriverError> {
    for cluster in cluster_chain(image, geom, first)? {
        set_fat_entry(image, geom, cluster, 0)?;
    }
    refresh_fsinfo(image, geom)?;
    Ok(())
}

/// Keep the FAT32 FSInfo free-cluster count consistent with the FAT.  Root
/// entries are shared by real formatters such as `mkfs.fat` and checked by
/// `fsck.fat`, so updating only the FAT is not sufficient.
fn refresh_fsinfo(image: &mut [u8], geom: Geometry) -> Result<(), DriverError> {
    if geom.reserved_sectors < 2 {
        return Ok(());
    }
    let free = free_cluster_count(image, geom)?;
    for sector in [1u32, 7u32] {
        if sector >= geom.reserved_sectors {
            continue;
        }
        let off = sector as usize * geom.bytes_per_sector as usize;
        let Some(sig) = image.get(off..off + 4) else {
            continue;
        };
        if u32::from_le_bytes([sig[0], sig[1], sig[2], sig[3]]) != 0x4161_5252 {
            continue;
        }
        let Some(free_slot) = image.get_mut(off + 488..off + 492) else {
            continue;
        };
        free_slot.copy_from_slice(&free.to_le_bytes());
        let Some(next_slot) = image.get_mut(off + 492..off + 496) else {
            continue;
        };
        next_slot.copy_from_slice(&0xffff_ffffu32.to_le_bytes());
    }
    Ok(())
}

fn write_chain(
    image: &mut [u8],
    geom: Geometry,
    chain: &[u32],
    data: &[u8],
) -> Result<(), DriverError> {
    let mut written = 0usize;
    for &cluster in chain {
        let off = cluster_offset(geom, cluster)?;
        let end = off + geom.cluster_size as usize;
        let dst = image.get_mut(off..end).ok_or(DriverError::IoError)?;
        dst.fill(0);
        let n = (data.len() - written).min(dst.len());
        dst[..n].copy_from_slice(&data[written..written + n]);
        written += n;
    }
    Ok(())
}

fn read_file_data(image: &[u8], geom: Geometry, entry: &RawEntry) -> Result<Vec<u8>, DriverError> {
    let mut out = Vec::with_capacity(entry.size as usize);
    if entry.size == 0 || entry.cluster == 0 {
        return Ok(out);
    }
    for cluster in cluster_chain(image, geom, entry.cluster)? {
        let off = cluster_offset(geom, cluster)?;
        let end = off + geom.cluster_size as usize;
        let block = image.get(off..end).ok_or(DriverError::IoError)?;
        let need = entry.size as usize - out.len();
        let n = need.min(block.len());
        out.extend_from_slice(&block[..n]);
        if out.len() >= entry.size as usize {
            break;
        }
    }
    if out.len() != entry.size as usize {
        return Err(DriverError::IoError);
    }
    Ok(out)
}

fn read_dir_chain(image: &[u8], geom: Geometry, cluster: u32) -> Result<Vec<u32>, DriverError> {
    cluster_chain(image, geom, cluster)
}

fn entries_in_dir(
    image: &[u8],
    geom: Geometry,
    dir_cluster: u32,
) -> Result<Vec<DirEntry>, DriverError> {
    let mut out = Vec::new();
    for cluster in read_dir_chain(image, geom, dir_cluster)? {
        let off = cluster_offset(geom, cluster)?;
        let end = off + geom.cluster_size as usize;
        let data = image.get(off..end).ok_or(DriverError::IoError)?;
        let mut lfn: Vec<(u8, String)> = Vec::new();
        for slot in 0..(geom.cluster_size as usize / 32) {
            let e = &data[slot * 32..slot * 32 + 32];
            if e[0] == 0 {
                return finish_entries(out);
            }
            if e[0] == 0xe5 {
                lfn.clear();
                continue;
            }
            let attr = e[11];
            if attr == ATTR_LFN {
                lfn.push((e[0] & 0x1f, lfn_chunk(e)));
                continue;
            }
            if attr & 0x08 != 0 {
                lfn.clear();
                continue;
            }
            let short = short_name(e);
            if short == "." || short == ".." {
                lfn.clear();
                continue;
            }
            let name = if lfn.is_empty() {
                short
            } else {
                lfn.sort_by_key(|(seq, _)| *seq);
                let mut s = String::new();
                for (_, part) in lfn.iter() {
                    s.push_str(part);
                }
                lfn.clear();
                if s.is_empty() {
                    short
                } else {
                    s
                }
            };
            out.push(DirEntry {
                name,
                is_dir: attr & ATTR_DIRECTORY != 0,
                size: u64::from(read_u32(e, 28)?),
            });
        }
    }
    finish_entries(out)
}

fn finish_entries(mut entries: Vec<DirEntry>) -> Result<Vec<DirEntry>, DriverError> {
    entries.retain(|e| e.name != "." && e.name != "..");
    Ok(entries)
}

fn lfn_chunk(e: &[u8]) -> String {
    let mut s = String::new();
    for range in [1..11, 14..26, 28..32] {
        for pair in e[range].chunks_exact(2) {
            let unit = u16::from_le_bytes([pair[0], pair[1]]);
            if unit == 0 || unit == 0xffff {
                continue;
            }
            s.push(core::char::from_u32(unit as u32).unwrap_or('\u{fffd}'));
        }
    }
    s
}

fn short_name(e: &[u8]) -> String {
    let mut base = String::new();
    for &b in &e[..8] {
        if b != b' ' {
            base.push((b as char).to_ascii_lowercase());
        }
    }
    let mut ext = String::new();
    for &b in &e[8..11] {
        if b != b' ' {
            ext.push((b as char).to_ascii_lowercase());
        }
    }
    if ext.is_empty() {
        base
    } else {
        base + "." + &ext
    }
}

fn find_entry(
    image: &[u8],
    geom: Geometry,
    dir_cluster: u32,
    wanted: &str,
) -> Result<Option<RawEntry>, DriverError> {
    for cluster in read_dir_chain(image, geom, dir_cluster)? {
        let base = cluster_offset(geom, cluster)?;
        let end = base + geom.cluster_size as usize;
        let data = image.get(base..end).ok_or(DriverError::IoError)?;
        let mut lfn: Vec<(u8, String)> = Vec::new();
        let mut lfn_start = base;
        for slot in 0..(geom.cluster_size as usize / 32) {
            let off = base + slot * 32;
            let e = &data[slot * 32..slot * 32 + 32];
            if e[0] == 0 {
                return Ok(None);
            }
            if e[0] == 0xe5 {
                lfn.clear();
                continue;
            }
            let attr = e[11];
            if attr == ATTR_LFN {
                if e[0] & 0x40 != 0 || lfn.is_empty() {
                    lfn_start = off;
                    lfn.clear();
                }
                lfn.push((e[0] & 0x1f, lfn_chunk(e)));
                continue;
            }
            if attr & 0x08 != 0 {
                lfn.clear();
                continue;
            }
            let short = short_name(e);
            // With no LFN entries the deleted range starts at the 8.3 entry
            // itself; `lfn_start` still points at the start of the directory
            // cluster and must not be used as the deletion base.
            let entry_start = if lfn.is_empty() { off } else { lfn_start };
            let long = if lfn.is_empty() {
                short.clone()
            } else {
                lfn.sort_by_key(|(seq, _)| *seq);
                let mut s = String::new();
                for (_, part) in lfn.iter() {
                    s.push_str(part);
                }
                lfn.clear();
                if s.is_empty() {
                    short.clone()
                } else {
                    s
                }
            };
            if long.eq_ignore_ascii_case(wanted) || short.eq_ignore_ascii_case(wanted) {
                return Ok(Some(RawEntry {
                    attr,
                    cluster: (u32::from(u16::from_le_bytes([e[20], e[21]])) << 16)
                        | u32::from(u16::from_le_bytes([e[26], e[27]])),
                    size: read_u32(e, 28)?,
                    start: entry_start,
                    end: off + 32,
                }));
            }
            lfn.clear();
        }
    }
    Ok(None)
}

fn resolve_dir(image: &[u8], geom: Geometry, path: &str) -> Result<u32, DriverError> {
    let path = normalize(path)?;
    let mut cluster = geom.root_cluster;
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        let entry = find_entry(image, geom, cluster, segment)?.ok_or(DriverError::NotFound)?;
        if entry.attr & ATTR_DIRECTORY == 0 {
            return Err(DriverError::NotFound);
        }
        cluster = entry.cluster;
    }
    Ok(cluster)
}

fn update_entry(
    image: &mut [u8],
    entry: &RawEntry,
    cluster: u32,
    size: u32,
) -> Result<(), DriverError> {
    let off = entry.end - 32;
    let dst = image.get_mut(off..off + 32).ok_or(DriverError::IoError)?;
    dst[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
    dst[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
    dst[28..32].copy_from_slice(&size.to_le_bytes());
    Ok(())
}

fn mark_deleted(image: &mut [u8], start: usize, end: usize) {
    let mut off = start;
    while off + 32 <= end && off < image.len() {
        image[off] = 0xe5;
        off += 32;
    }
}

fn make_dirty(image: &mut [u8], entry: &RawEntry) -> Result<(), DriverError> {
    let off = entry.end - 32;
    if let Some(attr) = image.get_mut(off + 11) {
        *attr |= ATTR_ARCHIVE;
    }
    Ok(())
}

fn create_entry(
    image: &mut [u8],
    geom: Geometry,
    parent: u32,
    name: &str,
    attr: u8,
    cluster: u32,
    size: u32,
) -> Result<usize, DriverError> {
    let (short, long) = make_names(image, geom, parent, name)?;
    let units: Vec<u16> = long.encode_utf16().collect();
    let short_display = short_array_display(&short);
    let lfn_count = if long.eq_ignore_ascii_case(&short_display) {
        0
    } else {
        units.len().div_ceil(13).max(1)
    };
    let slots = alloc_dir_slots(image, geom, parent, lfn_count + 1)?;
    let checksum = short_checksum(&short);
    if lfn_count > 0 {
        for (idx, &off) in slots.iter().enumerate().take(lfn_count) {
            let seq = lfn_count - idx;
            let entry = image.get_mut(off..off + 32).ok_or(DriverError::IoError)?;
            entry.fill(0);
            entry[0] = seq as u8 | if idx == 0 { 0x40 } else { 0 };
            entry[11] = ATTR_LFN;
            entry[13] = checksum;
            let start = (seq - 1) * 13;
            let mut positions = [1usize, 3, 5, 7, 9, 14, 16, 18, 20, 22, 24, 28, 30].into_iter();
            for n in 0..13 {
                let pos = positions.next().unwrap();
                let unit = units.get(start + n).copied().unwrap_or(
                    if n == units.len().saturating_sub(start) {
                        0
                    } else {
                        0xffff
                    },
                );
                entry[pos..pos + 2].copy_from_slice(&unit.to_le_bytes());
            }
        }
    }
    let off = slots[lfn_count];
    let entry = image.get_mut(off..off + 32).ok_or(DriverError::IoError)?;
    entry.fill(0);
    entry[..11].copy_from_slice(&short);
    entry[11] = attr;
    entry[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
    entry[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
    entry[28..32].copy_from_slice(&size.to_le_bytes());
    Ok(off)
}

fn alloc_dir_slots(
    image: &mut [u8],
    geom: Geometry,
    dir_cluster: u32,
    count: usize,
) -> Result<Vec<usize>, DriverError> {
    let mut chain = cluster_chain(image, geom, dir_cluster)?;
    let mut run: Vec<usize> = Vec::new();
    let mut index = 0usize;
    loop {
        if index >= chain.len() {
            let new_cluster = alloc_cluster(image, geom)?;
            let last = *chain.last().ok_or(DriverError::IoError)?;
            set_fat_entry(image, geom, last, new_cluster)?;
            chain.push(new_cluster);
            continue;
        }
        let cluster = chain[index];
        let base = cluster_offset(geom, cluster)?;
        for slot in 0..(geom.cluster_size as usize / 32) {
            let off = base + slot * 32;
            let first = image.get(off).copied().ok_or(DriverError::IoError)?;
            if first == 0 || first == 0xe5 {
                run.push(off);
                if run.len() == count {
                    return Ok(run);
                }
            } else {
                run.clear();
            }
        }
        index += 1;
    }
}

fn make_names(
    image: &[u8],
    geom: Geometry,
    parent: u32,
    name: &str,
) -> Result<([u8; 11], String), DriverError> {
    if name.is_empty() || name.len() > 255 {
        return Err(DriverError::InvalidName);
    }
    let upper = name.to_ascii_uppercase();
    let (base, ext) = upper.rsplit_once('.').unwrap_or((&upper, ""));
    let clean = |s: &str, max: usize| -> String {
        s.chars()
            .take(max)
            .map(|c| {
                if c.is_ascii_alphanumeric() || "$%'-_@~`!(){}^#&".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    };
    let mut base = clean(base, 8);
    let ext = clean(ext, 3);
    if base.is_empty() {
        base.push('_');
    }
    let mut candidate = short_array(&base, &ext);
    let mut n = 1u32;
    while short_exists(image, geom, parent, &candidate)? {
        let suffix = format!("~{}", n);
        let keep = 8usize.saturating_sub(suffix.len());
        let mut b: String = base.chars().take(keep).collect();
        b.push_str(&suffix);
        candidate = short_array(&b, &ext);
        n += 1;
        if n > 999_999 {
            return Err(DriverError::NoSpace);
        }
    }
    Ok((candidate, name.to_string()))
}

fn short_array(base: &str, ext: &str) -> [u8; 11] {
    let mut out = [b' '; 11];
    for (i, b) in base.as_bytes().iter().take(8).enumerate() {
        out[i] = *b;
    }
    for (i, b) in ext.as_bytes().iter().take(3).enumerate() {
        out[8 + i] = *b;
    }
    out
}

fn short_array_display(short: &[u8; 11]) -> String {
    let base = core::str::from_utf8(&short[..8]).unwrap_or("").trim_end();
    let ext = core::str::from_utf8(&short[8..]).unwrap_or("").trim_end();
    if ext.is_empty() {
        base.to_string()
    } else {
        format!("{}.{}", base, ext)
    }
}

fn short_exists(
    image: &[u8],
    geom: Geometry,
    dir_cluster: u32,
    short: &[u8; 11],
) -> Result<bool, DriverError> {
    for cluster in cluster_chain(image, geom, dir_cluster)? {
        let base = cluster_offset(geom, cluster)?;
        for slot in 0..(geom.cluster_size as usize / 32) {
            let off = base + slot * 32;
            let e = image.get(off..off + 32).ok_or(DriverError::IoError)?;
            if e[0] == 0 {
                return Ok(false);
            }
            if e[0] != 0xe5 && e[11] != ATTR_LFN && &e[..11] == short {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn short_checksum(short: &[u8; 11]) -> u8 {
    let mut sum = 0u8;
    for &b in short {
        sum = sum.rotate_right(1).wrapping_add(b);
    }
    sum
}

fn initialize_directory(
    image: &mut [u8],
    geom: Geometry,
    off: usize,
    cluster: u32,
    parent: u32,
) -> Result<(), DriverError> {
    let end = off + geom.cluster_size as usize;
    image.get_mut(off..end).ok_or(DriverError::IoError)?.fill(0);
    // FAT requires the `..` entry of a directory directly below the root to
    // use cluster 0 rather than the root cluster itself.
    let parent_cluster = if parent == geom.root_cluster {
        0
    } else {
        parent
    };
    for (slot, name, target) in [
        (0usize, *b".          ", cluster),
        (1usize, *b"..         ", parent_cluster),
    ] {
        let e = image
            .get_mut(off + slot * 32..off + slot * 32 + 32)
            .ok_or(DriverError::IoError)?;
        e[..11].copy_from_slice(&name);
        e[11] = ATTR_DIRECTORY;
        e[20..22].copy_from_slice(&((target >> 16) as u16).to_le_bytes());
        e[26..28].copy_from_slice(&(target as u16).to_le_bytes());
    }
    Ok(())
}

fn find_dotdot_offset(
    image: &[u8],
    geom: Geometry,
    cluster: u32,
) -> Result<Option<usize>, DriverError> {
    let off = cluster_offset(geom, cluster)?;
    let e = image.get(off + 32..off + 64).ok_or(DriverError::IoError)?;
    if e[..2] == *b".." {
        Ok(Some(off + 32))
    } else {
        Ok(None)
    }
}

fn free_cluster_count(image: &[u8], geom: Geometry) -> Result<u32, DriverError> {
    let mut free = 0;
    for cluster in 2..geom.cluster_count + 2 {
        if fat_entry(image, geom, cluster)? == 0 {
            free += 1;
        }
    }
    Ok(free)
}

fn format_image(total_bytes: usize, sectors_per_cluster: u32) -> Result<Vec<u8>, DriverError> {
    const BPS: u32 = 512;
    const RESERVED: u32 = 32;
    const NFATS: u32 = 2;
    if total_bytes < 1024 * 1024 || total_bytes % BPS as usize != 0 {
        return Err(DriverError::InvalidArgument);
    }
    let total_sectors = (total_bytes / BPS as usize) as u32;
    let mut fat_size = 1u32;
    let mut clusters = 0u32;
    for _ in 0..8 {
        let new_clusters = total_sectors.saturating_sub(RESERVED + NFATS * fat_size);
        let new_fat = ((u64::from(new_clusters) + 2) * 4).div_ceil(u64::from(BPS)) as u32;
        if new_fat == fat_size {
            clusters = new_clusters;
            break;
        }
        fat_size = new_fat;
        clusters = new_clusters;
    }
    if clusters < 65_525 {
        return Err(DriverError::InvalidArgument);
    }
    let data_start = RESERVED + NFATS * fat_size;
    let mut image = vec![0u8; total_bytes];
    {
        let boot = &mut image[..512];
        boot[0..3].copy_from_slice(&[0xeb, 0x3c, 0x90]);
        boot[3..11].copy_from_slice(b"CAPLET  ");
        boot[11..13].copy_from_slice(&(BPS as u16).to_le_bytes());
        boot[13] = sectors_per_cluster as u8;
        boot[14..16].copy_from_slice(&(RESERVED as u16).to_le_bytes());
        boot[16] = NFATS as u8;
        boot[21] = 0xf8;
        boot[24..26].copy_from_slice(&32u16.to_le_bytes());
        boot[26..28].copy_from_slice(&255u16.to_le_bytes());
        boot[32..36].copy_from_slice(&total_sectors.to_le_bytes());
        boot[36..40].copy_from_slice(&fat_size.to_le_bytes());
        boot[44..48].copy_from_slice(&2u32.to_le_bytes());
        boot[48..50].copy_from_slice(&1u16.to_le_bytes());
        boot[50..52].copy_from_slice(&6u16.to_le_bytes());
        boot[64] = 0x80;
        boot[66] = 0x29;
        boot[67..71].copy_from_slice(&0x4341_504c_u32.to_le_bytes());
        boot[71..82].copy_from_slice(b"CAPLET     ");
        boot[82..90].copy_from_slice(b"FAT32   ");
        boot[510] = 0x55;
        boot[511] = 0xaa;
    }
    for sector in [1u32, 7] {
        let off = sector as usize * 512;
        let f = &mut image[off..off + 512];
        f[0..4].copy_from_slice(&0x4161_5254u32.to_le_bytes());
        f[484..488].copy_from_slice(&0x6141_5254u32.to_le_bytes());
        f[488..492].copy_from_slice(&(clusters.saturating_sub(1)).to_le_bytes());
        f[492..496].copy_from_slice(&3u32.to_le_bytes());
        f[510] = 0x55;
        f[511] = 0xaa;
    }
    let boot_copy = image[..512].to_vec();
    image[6 * 512..7 * 512].copy_from_slice(&boot_copy);
    let geom = Geometry {
        bytes_per_sector: BPS,
        reserved_sectors: RESERVED,
        num_fats: NFATS,
        fat_size,
        root_cluster: 2,
        fat_start: RESERVED * BPS,
        data_start: data_start * BPS,
        cluster_count: clusters,
        cluster_size: sectors_per_cluster * BPS,
    };
    for copy in 0..NFATS {
        let base = geom.fat_start as usize + copy as usize * fat_size as usize * BPS as usize;
        write_u32(&mut image, base, 0x0fff_fff8)?;
        write_u32(&mut image, base + 4, 0x0fff_ffff)?;
        write_u32(&mut image, base + 8, FAT_EOC)?;
    }
    Ok(image)
}

impl crate::powercut::PowercutAware for Fat32 {
    fn checkpoint_bytes(&self) -> Vec<u8> {
        self.image_bytes()
    }

    fn restore_from_checkpoint(bytes: &[u8]) -> Self {
        Self::try_restore_from_checkpoint(bytes).unwrap_or_else(|_| {
            Self::format(64 * 1024 * 1024, 1).expect("format recovery FAT32 volume")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::powercut::PowercutAware;

    fn volume() -> Fat32 {
        Fat32::format(64 * 1024 * 1024, 1).unwrap()
    }

    #[test]
    fn format_and_persist_file() {
        let fs = volume();
        let h = fs.open("/sentinel.txt", FileMode::CreateTruncate).unwrap();
        fs.write(h, b"fat32-persist").unwrap();
        fs.flush(h).unwrap();
        fs.close(h).unwrap();
        let bytes = fs.checkpoint_bytes();
        let restored = Fat32::try_restore_from_checkpoint(&bytes).unwrap();
        let h = restored.open("/sentinel.txt", FileMode::Read).unwrap();
        let mut b = [0u8; 32];
        let n = restored.read(h, &mut b).unwrap();
        assert_eq!(&b[..n], b"fat32-persist");
        restored.close(h).unwrap();
    }

    #[test]
    fn long_names_dirs_rename_and_delete() {
        let fs = volume();
        fs.mkdir("/logs").unwrap();
        let h = fs
            .open("/logs/a-very-long-filename.log", FileMode::CreateTruncate)
            .unwrap();
        fs.write(h, b"hello").unwrap();
        fs.close(h).unwrap();
        let names = fs.read_dir("/logs").unwrap();
        assert_eq!(names[0].name, "a-very-long-filename.log");
        fs.rename("/logs/a-very-long-filename.log", "/logs/renamed.log")
            .unwrap();
        let h = fs.open("/logs/renamed.log", FileMode::Read).unwrap();
        let mut b = [0u8; 5];
        assert_eq!(fs.read(h, &mut b).unwrap(), 5);
        assert_eq!(&b, b"hello");
        fs.close(h).unwrap();
        fs.unlink("/logs/renamed.log").unwrap();
        fs.rmdir("/logs").unwrap();
    }

    #[test]
    fn statvfs_reports_cluster_geometry() {
        let fs = volume();
        let (total, free) = fs.statvfs().unwrap();
        assert!(total <= 64 * 1024 * 1024);
        assert!(total > 60 * 1024 * 1024);
        assert_eq!(total % 512, 0);
        assert!(free <= total);
        assert!(free > total / 2);
    }
}
