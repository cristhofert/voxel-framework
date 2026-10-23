//! Inventory serialisation and deserialisation to/from JSON files on disk.
//!
//! This module provides two free functions — [`save_inventory`] and
//! [`load_inventory`] — that convert an [`crate::items::inventory::Inventory`]
//! to and from a human-readable JSON file.  The format is intentionally simple
//! and hand-editable.
//!
//! # File format
//!
//! The JSON file is a single object with two top-level fields:
//!
//! ```json
//! {
//!   "capacity": 3,
//!   "slots": [
//!     null,
//!     { "item_byte": 2, "count": 7 },
//!     null
//!   ]
//! }
//! ```
//!
//! * `capacity` — the maximum slot count of the original inventory.
//! * `slots` — one entry per slot; `null` for empty slots, or an object with
//!   `item_byte` (the value returned by [`crate::items::inventory::ItemType::to_byte`])
//!   and `count`.
//!
//! # Save / load contract
//!
//! * [`save_inventory`] validates the source and decoded byte-alias totals, creates missing parent directories,
//!   and replaces the destination with a synced sibling temporary file. Errors
//!   before replacement leave an existing save untouched.
//! * [`load_inventory`] restores the inventory with the same `capacity` and
//!   slot layout. Zero counts and bytes decoded as default/empty become `None`
//!   at the same index; malformed layout or overflowing item totals are rejected.
//! * Existing regular-file permissions are preserved. Final symlinks are followed
//!   without replacing the link, on both save and load. Follow-through resolves
//!   the link manually (see `resolved_link_target`) instead of letting the
//!   kernel open through the reparse point: CI runners whose `%TEMP%` mixes the
//!   8.3 short name (`RUNNER~1`) with long names return `ERROR_INVALID_NAME`
//!   (123) exactly when opening through a relative-target link. Dangling links,
//!   nonregular/read-only targets, and Unix multiply hard-linked targets are
//!   rejected.
//! * Replacement provides whole-snapshot visibility, not transaction isolation
//!   between writers or universal power-cut durability. Parent directories must
//!   be trusted: hostile namespace races and preservation of existing ownership,
//!   ACLs, and extended attributes are outside this contract. New Windows files
//!   use a protected current-user-only DACL, verified before any JSON write;
//!   filesystems without persistent ACL support fail closed.
//! * Both functions propagate I/O and JSON errors as `Box<dyn std::error::Error>`.

use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

use super::inventory::{Inventory, InventorySlot, ItemType};

// ─── On-disk format ──────────────────────────────────────────────────────────

/// Serialisable representation of an [`InventorySlot`].
#[derive(Serialize, Deserialize)]
struct SlotDto {
    item_byte: u8,
    count: u32,
}

/// Serialisable representation of an [`Inventory`].
#[derive(Serialize, Deserialize)]
struct InventoryDto {
    capacity: usize,
    slots: Vec<Option<SlotDto>>,
}

// ─── save_inventory ───────────────────────────────────────────────────────────

/// Writes `inventory` to a pretty-printed JSON file at `path`.
///
/// Creates the file and any missing parent directories.  If the file already
/// exists it is replaced atomically after writing and syncing a sibling temporary
/// file. Pre-replacement failures preserve the old bytes and remove the temporary
/// file. Existing permissions and final symlinks are preserved; new files retain
/// private creation permissions (0600 on Unix; protected current-user DACL on
/// Windows, verified before writing data). No old-file deletion or direct-write
/// fallback is used. See the module contract for metadata/durability limitations.
///
/// # Parameters
///
/// * `inventory` — the [`crate::items::inventory::Inventory`] to serialise.
/// * `path` — filesystem path for the output file (e.g. `"saves/inventory.json"`).
///
/// # Errors
///
/// Returns a boxed error if:
/// * The inventory shape, live stacks, runtime or decoded byte-alias totals are invalid
///   ([`std::io::ErrorKind::InvalidInput`]).
/// * The target is dangling, nonregular, read-only, or multiply hard-linked on Unix.
/// * A parent directory cannot be created ([`std::io::Error`]).
/// * The JSON cannot be serialised ([`serde_json::Error`]).
/// * The file cannot be written ([`std::io::Error`]).
///
/// # Examples
///
/// ```rust,no_run
/// use voxel_framework::items::{DefaultItem, Inventory, save_inventory};
///
/// let mut inv = Inventory::<DefaultItem>::new(36);
/// inv.add(DefaultItem::IronOre, 5);
/// save_inventory(&inv, "saves/player.json").expect("failed to save inventory");
/// ```
pub fn save_inventory<I: ItemType + Serialize>(
    inventory: &Inventory<I>,
    path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    save_inventory_with_ops(inventory, Path::new(path), &mut FileWriteOps)
}

fn save_inventory_with_ops<I: ItemType + Serialize>(
    inventory: &Inventory<I>,
    path: &Path,
    ops: &mut impl AtomicWriteOps,
) -> Result<(), Box<dyn std::error::Error>> {
    inventory
        .validate_integrity()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid inventory source"))?;
    let dto = InventoryDto {
        capacity: inventory.capacity,
        slots: inventory
            .slots
            .iter()
            .map(|slot| {
                slot.as_ref().map(|s| SlotDto {
                    item_byte: s.item.to_byte(),
                    count: s.count,
                })
            })
            .collect(),
    };

    // Byte aliases can collapse distinct runtime items into one decoded item.
    // Validate exactly what load will reconstruct before touching the filesystem.
    decode_inventory::<I>(&dto, io::ErrorKind::InvalidInput)?;
    let json = serde_json::to_vec_pretty(&dto)?;
    let (target, permissions) = prepare_target(path)?;
    let mut temp = private_tempfile(parent_directory(&target))?;
    ops.check_private(temp.as_file())?;
    ops.write_all(temp.as_file_mut(), &json)?;
    if let Some(permissions) = permissions {
        if let Err(error) = temp.as_file().set_permissions(permissions) {
            return Err(error.into());
        }
    }
    ops.sync_all(temp.as_file())?;
    ops.replace(temp, &target)?;
    Ok(())
}

fn parent_directory(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn prepare_target(path: &Path) -> io::Result<(PathBuf, Option<Permissions>)> {
    // symlink_metadata distinguishes an absent destination from a dangling link.
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(parent_directory(path))?;
            return Ok((path.to_path_buf(), None));
        }
        Err(error) => return Err(error),
    };
    let target = if metadata.file_type().is_symlink() {
        resolved_link_target(path)?
    } else {
        path.to_path_buf()
    };
    let metadata = match fs::metadata(&target) {
        Ok(metadata) => metadata,
        Err(error) => return Err(error),
    };
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "inventory destination must be a regular file",
        ));
    }
    let permissions = metadata.permissions();
    if permissions.readonly() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "inventory destination is read-only",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() > 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "inventory destination has multiple hard links",
            ));
        }
    }
    // Replacement needs parent access, but also honor the existing file's write
    // access. This probe never truncates and is closed before Windows replacement.
    drop(OpenOptions::new().write(true).open(&target)?);
    Ok((target, Some(permissions)))
}

/// Resolves a final-path symlink to the regular file it targets.
///
/// `fs::canonicalize` is deliberately avoided here. On the Windows CI runner
/// `%TEMP%` mixes the 8.3 short name (`RUNNER~1`) with long-name components,
/// and canonicalizing a path that also crosses a relative symlink target is
/// the one place `GetFinalPathNameByHandleW` fails with `ERROR_INVALID_NAME`
/// (123). The target is instead resolved manually: the link text is read
/// verbatim, joined onto the link's directory when relative, and then cleaned
/// purely lexically (`.` dropped, `..` collapsed against the component stack).
/// No kernel round-trip, no case rewriting, no verbatim re-encoding. Lexical
/// normalization is semantically exact here because every component before a
/// `..` is a real directory — `prepare_target` already verified the link
/// parent with `symlink_metadata` before resolving.
#[cfg(windows)]
fn resolved_link_target(path: &Path) -> io::Result<PathBuf> {
    let raw = fs::read_link(path)?;
    let joined = if raw.is_absolute() {
        raw
    } else {
        parent_directory(path).join(raw)
    };
    Ok(lexically_normalize_windows(&joined))
}

/// Lexically normalizes a Windows path: drops `.` components, resolves `..`
/// against the component stack, and never pops past the prefix or root. Drive
/// and UNC prefixes are preserved with their original case.
#[cfg(windows)]
fn lexically_normalize_windows(path: &Path) -> PathBuf {
    use std::ffi::OsString;
    use std::path::Component;

    let mut prefix = PathBuf::new();
    let mut absolute = false;
    let mut stack: Vec<OsString> = Vec::new();
    // Leading/remaining `..` of a relative path (e.g. `a/../../b` → `../b`).
    let mut parents = 0usize;
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                absolute = true;
                prefix.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if stack.pop().is_some() {
                    // Consumed one real component; the `..` is resolved.
                } else if absolute {
                    // Above the root: meaningless for absolute paths, drop it.
                } else {
                    parents += 1;
                }
            }
            Component::Normal(part) => stack.push(part.to_os_string()),
        }
    }
    let mut normalized = prefix;
    for _ in 0..parents {
        normalized.push("..");
    }
    for part in &stack {
        normalized.push(part);
    }
    normalized
}

/// Resolves a final-path symlink to the regular file it targets.
#[cfg(not(windows))]
fn resolved_link_target(path: &Path) -> io::Result<PathBuf> {
    fs::canonicalize(path)
}

// Private adapter keeps failure injection off the public API. replace consumes
// the temporary file so both injected errors and real PersistError paths clean up.
trait AtomicWriteOps {
    fn check_private(&mut self, file: &File) -> io::Result<()> {
        #[cfg(windows)]
        windows_private::verify_private(file)?;
        #[cfg(not(windows))]
        let _ = file;
        Ok(())
    }
    fn write_all(&mut self, file: &mut File, bytes: &[u8]) -> io::Result<()>;
    fn sync_all(&mut self, file: &File) -> io::Result<()>;
    fn replace(&mut self, temp: NamedTempFile, target: &Path) -> io::Result<()>;
}

struct FileWriteOps;

fn private_tempfile(parent: &Path) -> io::Result<NamedTempFile> {
    #[cfg(windows)]
    {
        windows_private::tempfile(parent)
    }
    #[cfg(not(windows))]
    {
        NamedTempFile::new_in(parent)
    }
}

impl AtomicWriteOps for FileWriteOps {
    fn write_all(&mut self, file: &mut File, bytes: &[u8]) -> io::Result<()> {
        file.write_all(bytes)
    }

    fn sync_all(&mut self, file: &File) -> io::Result<()> {
        file.sync_all()
    }

    fn replace(&mut self, temp: NamedTempFile, target: &Path) -> io::Result<()> {
        #[cfg(windows)]
        {
            windows_private::replace(temp, target)
        }
        #[cfg(not(windows))]
        {
            // Moving out the I/O error drops the PersistError-owned temporary file.
            temp.persist(target)
                .map(|_| ())
                .map_err(|error| error.error)
        }
    }
}

// Keep native security operations isolated: no path-based ACL repair after
// creation, no inherited/default DACL, and no confidential write before readback.
#[cfg(windows)]
mod windows_private {
    use super::*;
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{
        ERROR_NO_TOKEN, ERROR_SUCCESS, INVALID_HANDLE_VALUE, LocalFree,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        GetSecurityInfo, SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACL, DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetLengthSid,
        GetSecurityDescriptorControl, GetSecurityDescriptorDacl, GetTokenInformation, IsValidAcl,
        IsValidSecurityDescriptor, IsValidSid, SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, TOKEN_QUERY,
        TOKEN_USER, TokenUser,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CREATE_NEW, CreateFileW, DELETE, FILE_ALL_ACCESS, FILE_ATTRIBUTE_NORMAL,
        FILE_ATTRIBUTE_TEMPORARY, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, GetVolumeInformationByHandleW, SetFileAttributesW,
    };
    use windows_sys::Win32::System::SystemServices::{
        ACCESS_ALLOWED_ACE_TYPE, FILE_PERSISTENT_ACLS,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken,
    };

    // Only owns buffers allocated by the Windows APIs documented to use LocalFree.
    struct LocalBuffer(*mut c_void);

    impl Drop for LocalBuffer {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: this unique allocation came from a LocalAlloc-family
                // security API; no borrowed pointers survive this owner.
                unsafe { LocalFree(self.0) };
            }
        }
    }

    fn denied(message: &'static str) -> io::Error {
        io::Error::new(io::ErrorKind::PermissionDenied, message)
    }

    fn descriptor(sddl: &str) -> io::Result<LocalBuffer> {
        let wide: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        let mut pointer = null_mut();
        // SAFETY: wide is terminated and alive for the call; pointer is writable.
        // Windows allocates a self-relative descriptor, owned immediately below.
        let result = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut pointer,
                null_mut(),
            )
        };
        let buffer = LocalBuffer(pointer);
        if result == 0 {
            return Err(io::Error::last_os_error());
        }
        if buffer.0.is_null() {
            return Err(denied("Windows returned no security descriptor"));
        }
        Ok(buffer)
    }

    fn user_descriptor() -> io::Result<LocalBuffer> {
        let sid = effective_user_sid()?;
        descriptor(&format!("D:P(A;;FA;;;{sid})"))
    }

    fn effective_user_sid() -> io::Result<String> {
        let mut handle = null_mut();
        // SAFETY: pseudo-handle is valid for this thread; output is writable.
        // Query the effective impersonation user, not an assumed process identity.
        let opened = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut handle) };
        if opened == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_NO_TOKEN as i32) {
                return Err(error);
            }
            // SAFETY: process pseudo-handle and writable output are valid;
            // fallback is allowed only when the thread has no token.
            if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) } == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        // SAFETY: successful Open*Token transfers one valid owned handle to us.
        let token = unsafe { OwnedHandle::from_raw_handle(handle) };
        let mut needed = 0;
        // SAFETY: null/zero requests the required size; needed is writable.
        unsafe {
            GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut needed)
        };
        if needed < size_of::<TOKEN_USER>() as u32 {
            return Err(denied("Windows returned invalid token-user size"));
        }
        // usize storage provides TOKEN_USER's pointer alignment, unlike Vec<u8>.
        let mut data = vec![0usize; (needed as usize).div_ceil(size_of::<usize>())];
        let mut returned = 0;
        // SAFETY: aligned data holds at least needed bytes and remains alive
        // while the TOKEN_USER and its interior SID pointer are used below.
        if unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                data.as_mut_ptr().cast(),
                needed,
                &mut returned,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if returned < size_of::<TOKEN_USER>() as u32 || returned > needed {
            return Err(denied("Windows returned invalid token-user data"));
        }
        // SAFETY: successful TokenUser filled an aligned TOKEN_USER structure;
        // Windows supplies its valid SID within data, which is still alive.
        let user = unsafe { &*data.as_ptr().cast::<TOKEN_USER>() };
        let mut pointer = null_mut();
        // SAFETY: user SID is API-provided and alive; output is writable.
        let result = unsafe { ConvertSidToStringSidW(user.User.Sid, &mut pointer) };
        let text = LocalBuffer(pointer.cast());
        if result == 0 {
            return Err(io::Error::last_os_error());
        }
        if text.0.is_null() {
            return Err(denied("Windows returned no user SID"));
        }
        let mut length = 0;
        // SAFETY: successful conversion returns a terminated UTF-16 allocation;
        // text owns it through scanning/copying and releases it afterward.
        unsafe {
            while *pointer.add(length) != 0 {
                length += 1;
            }
            String::from_utf16(std::slice::from_raw_parts(pointer, length))
                .map_err(|_| denied("Windows returned invalid user SID text"))
        }
    }

    pub(super) fn tempfile(parent: &Path) -> io::Result<NamedTempFile> {
        let security = user_descriptor()?;
        // make_in owns cleanup immediately after the callback creates the file.
        // Readback checks run only afterward, so an error cannot orphan a temp.
        tempfile::Builder::new().make_in(parent, |path| create(path, &security))
    }

    fn create(path: &Path, security: &LocalBuffer) -> io::Result<File> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if wide[..wide.len() - 1].contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "NUL in inventory path",
            ));
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security.0,
            bInheritHandle: 0,
        };
        // SAFETY: terminated path, attributes and valid self-relative descriptor
        // remain alive through CreateFileW. CREATE_NEW never opens/truncates an
        // existing file. Protected DACL excludes parent ACEs from creation.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                &attributes,
                CREATE_NEW,
                FILE_ATTRIBUTE_TEMPORARY,
                null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful CreateFileW transfers one owned file handle.
        Ok(unsafe { File::from_raw_handle(handle) })
    }

    pub(super) fn replace(temp: NamedTempFile, target: &Path) -> io::Result<()> {
        let persist = match temp.persist(target) {
            Ok(_file) => return Ok(()),
            Err(error) => error,
        };
        // Concurrent writers racing to replace the same destination can make
        // MoveFileExW fail with ERROR_ACCESS_DENIED / ERROR_SHARING_VIOLATION
        // while another thread's replace has the target transiently open.
        // Recovering through std::fs::rename (which retries with a DELETE-access
        // handle and FileRenameInfoEx POSIX semantics) matches how std resolves
        // exactly this race. The PersistError still owns the temporary file, so
        // any fallback failure preserves the original error and cleans it up.
        if !matches!(
            persist.error.kind(),
            io::ErrorKind::PermissionDenied
                | io::ErrorKind::InvalidFilename
                | io::ErrorKind::WouldBlock
        ) {
            return Err(persist.error);
        }
        let replace = replace_fallback(persist.file.path(), target);
        // After a successful fallback the temporary path no longer exists, so
        // dropping the file is just cleanup of a stale path. On failure the
        // tempfile drop removes the still-existing temporary file.
        drop(persist.file);
        if replace.is_ok() {
            Ok(())
        } else {
            Err(persist.error)
        }
    }

    // Mirrors tempfile's persist ordering (clears the temporary-file marker
    // first), then performs the replacement through a file-handle rename that
    // bypasses MoveFileExW altogether. MoveFileExW rejects the destination
    // strings derived from canonicalized paths with ERROR_INVALID_NAME even
    // after the verbatim prefix is stripped, and std::fs::rename only retries
    // that error case for ERROR_ACCESS_DENIED, so both string-based renames are
    // useless here. SetFileInformationByHandle(FileRenameInfoEx) renames the
    // already-open temporary file onto the target with REPLACE_IF_EXISTS and
    // POSIX semantics — the same mechanism std uses for its access-denied
    // retry — and only the destination is parsed as a path string.
    fn replace_fallback(path: &Path, target: &Path) -> io::Result<()> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: terminated path buffer stays alive for the call; the file
        // still exists at the temporary path the PersistError preserved.
        if unsafe { SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_NORMAL) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // DELETE is the only access a rename source requires; the flags keep
        // the open working on any attributes and reparse points, matching how
        // std opens rename sources before retrying with FileRenameInfoEx.
        let handle = OpenOptions::new()
            .access_mode(DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        rename_with_posix_semantics(&handle, target)
    }

    // Renames the open source handle onto `target`, replacing an existing file.
    // Mirrors std's rename fallback layout for the dynamically sized
    // FILE_RENAME_INFO: offset_of(FileName) + UTF-16 name bytes + the struct's
    // trailing [u16; 1] placeholder.
    fn rename_with_posix_semantics(source: &File, target: &Path) -> io::Result<()> {
        use std::mem::offset_of;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_RENAME_INFO, FileRenameInfoEx, SetFileInformationByHandle,
        };
        const FILE_RENAME_FLAG_REPLACE_IF_EXISTS: u32 = 0x0000_0001;
        const FILE_RENAME_FLAG_POSIX_SEMANTICS: u32 = 0x0000_0002;
        let name: Vec<u16> = target.as_os_str().encode_wide().collect();
        // SAFETY: a NUL-free UTF-16 length cannot overflow u32 for any path the
        // filesystem accepts (32k total wide characters at most).
        let name_bytes = u32::try_from(name.len() * size_of::<u16>())
            .map_err(|_| io::Error::from_raw_os_error(123))?;
        let offset = offset_of!(FILE_RENAME_INFO, FileName) as u32;
        let struct_size = offset + name_bytes + 2;
        // u64 storage provides FILE_RENAME_INFO's alignment; the rounded buffer
        // holds the header plus every name byte.
        let mut info = vec![0u64; (struct_size as usize).div_ceil(size_of::<u64>())];
        let header = info.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        // SAFETY: info is aligned with room for the header and the full name;
        // the kernel reads exactly FileNameLength bytes from the trailing array.
        unsafe {
            (*header).Anonymous.Flags =
                FILE_RENAME_FLAG_REPLACE_IF_EXISTS | FILE_RENAME_FLAG_POSIX_SEMANTICS;
            (*header).RootDirectory = null_mut();
            (*header).FileNameLength = name_bytes;
            name.as_ptr()
                .copy_to_nonoverlapping((*header).FileName.as_mut_ptr(), name.len());
        }
        // SAFETY: source is a live eligible handle with DELETE; info describes
        // the rename and stays valid for the call.
        if unsafe {
            SetFileInformationByHandle(
                source.as_raw_handle(),
                FileRenameInfoEx,
                info.as_ptr().cast(),
                struct_size,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // The temp was created with FILE_ATTRIBUTE_TEMPORARY and the failed
        // persist restored that marker on it, so the replaced file must not
        // keep the temporary-file hint. Best-effort: failure leaves the hint.
        let target_wide: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: terminated path; best-effort normalization of the new file.
        unsafe { SetFileAttributesW(target_wide.as_ptr(), FILE_ATTRIBUTE_NORMAL) };
        Ok(())
    }

    pub(super) fn verify_private(file: &File) -> io::Result<()> {
        let expected = user_descriptor()?;
        verify_against(file, &expected)
    }

    fn require_acl_support(flags: u32) -> io::Result<()> {
        if flags & FILE_PERSISTENT_ACLS == 0 {
            return Err(denied(
                "inventory volume does not support persistent private ACLs",
            ));
        }
        Ok(())
    }

    fn verify_against(file: &File, expected: &LocalBuffer) -> io::Result<()> {
        let mut flags = 0;
        // SAFETY: file owns a live handle; unused outputs may be null/zero;
        // flags is writable. The queried volume is the actual open temp's volume.
        if unsafe {
            GetVolumeInformationByHandleW(
                file.as_raw_handle(),
                null_mut(),
                0,
                null_mut(),
                null_mut(),
                &mut flags,
                null_mut(),
                0,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        require_acl_support(flags)?;
        let actual = file_descriptor(file)?;
        let actual_ace = single_private_ace(&actual)?;
        let expected_ace = single_private_ace(expected)?;
        // SAFETY: validated API-produced descriptors own both ACEs/SIDs;
        // allocations stay alive until comparison is complete.
        if unsafe { EqualSid(actual_ace.1, expected_ace.1) } == 0 {
            return Err(denied(
                "inventory DACL does not belong exclusively to current user",
            ));
        }
        Ok(())
    }

    fn file_descriptor(file: &File) -> io::Result<LocalBuffer> {
        let mut pointer = null_mut();
        // SAFETY: file handle is live with READ_CONTROL (FILE_GENERIC_READ);
        // optional outputs may be null; pointer receives a LocalFree allocation.
        let result = unsafe {
            GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut pointer,
            )
        };
        let security = LocalBuffer(pointer);
        if result != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(result as i32));
        }
        if security.0.is_null() {
            return Err(denied("Windows returned no file security descriptor"));
        }
        Ok(security)
    }

    fn single_private_ace(security: &LocalBuffer) -> io::Result<(u32, *mut c_void)> {
        let mut control = 0;
        let mut revision = 0;
        let mut present = 0;
        let mut defaulted = 0;
        let mut acl: *mut ACL = null_mut();
        // SAFETY: security owns a nonnull descriptor returned by Windows; all
        // outputs are writable. Validate descriptor/ACL before dereferencing.
        unsafe {
            if IsValidSecurityDescriptor(security.0) == 0
                || GetSecurityDescriptorControl(security.0, &mut control, &mut revision) == 0
                || control & SE_DACL_PROTECTED == 0
                || GetSecurityDescriptorDacl(security.0, &mut present, &mut acl, &mut defaulted)
                    == 0
                || present == 0
                || defaulted != 0
                || acl.is_null()
                || IsValidAcl(acl) == 0
            {
                return Err(denied("inventory DACL is absent, invalid or unprotected"));
            }
            if (*acl).AceCount != 1 {
                return Err(denied("inventory DACL must contain exactly one user ACE"));
            }
            let mut pointer = null_mut();
            if GetAce(acl, 0, &mut pointer) == 0 || pointer.is_null() {
                return Err(denied("inventory user ACE is unavailable"));
            }
            // GetAce points into the validated, API-owned ACL. Inspect the
            // common header before interpreting it as an ACCESS_ALLOWED_ACE.
            let header = &*pointer.cast::<windows_sys::Win32::Security::ACE_HEADER>();
            if header.AceType != ACCESS_ALLOWED_ACE_TYPE as u8
                || header.AceFlags != 0
                || (header.AceSize as usize) < size_of::<ACCESS_ALLOWED_ACE>() + 4
            {
                return Err(denied(
                    "inventory ACE is inherited or not a simple user allow",
                ));
            }
            let ace = &*pointer.cast::<ACCESS_ALLOWED_ACE>();
            if ace.Mask != FILE_ALL_ACCESS {
                return Err(denied("inventory user ACE has unexpected access rights"));
            }
            // SidStart marks the variable-length SID in a validated allow ACE.
            let sid = std::ptr::addr_of!(ace.SidStart).cast_mut().cast();
            if GetLengthSid(sid) as usize > header.AceSize as usize - 8 || IsValidSid(sid) == 0 {
                return Err(denied("inventory user ACE contains an invalid SID"));
            }
            Ok((ace.Mask, sid))
        }
    }

    #[cfg(test)]
    pub(super) fn reject_unsupported_volume() -> io::Result<()> {
        require_acl_support(0)
    }

    #[cfg(test)]
    pub(super) fn set_test_dacl(file: &File, sddl: &str) -> io::Result<()> {
        use windows_sys::Win32::Security::Authorization::SetSecurityInfo;
        use windows_sys::Win32::Security::PROTECTED_DACL_SECURITY_INFORMATION;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, READ_CONTROL, ReOpenFile, WRITE_DAC,
        };
        let security = descriptor(sddl)?;
        let mut present = 0;
        let mut defaulted = 0;
        let mut acl = null_mut();
        // SAFETY: parsed descriptor is valid and alive; all outputs are writable.
        if unsafe { GetSecurityDescriptorDacl(security.0, &mut present, &mut acl, &mut defaulted) }
            == 0
            || present == 0
            || acl.is_null()
        {
            return Err(denied("test DACL missing"));
        }
        let flags = if file.metadata()?.is_dir() {
            FILE_FLAG_BACKUP_SEMANTICS
        } else {
            0
        };
        // SAFETY: live file handle; test requests rights granted by the test's
        // user/full-control ACL. Newly returned handle is owned below.
        let handle = unsafe {
            ReOpenFile(
                file.as_raw_handle(),
                READ_CONTROL | WRITE_DAC,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                flags,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful ReOpenFile returns one owned handle.
        let writable = unsafe { OwnedHandle::from_raw_handle(handle) };
        // SAFETY: handle has WRITE_DAC; ACL is valid and alive. Tests change only
        // their own files/directories; no SACL, owner or privilege changes.
        let result = unsafe {
            SetSecurityInfo(
                writable.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null_mut(),
            )
        };
        if result != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(result as i32));
        }
        Ok(())
    }

    /// Test-only variant of [`set_test_dacl`] that applies the DACL directly on
    /// an already opened handle. ReOpenFile can never produce a handle with
    /// access rights beyond those granted to the original open and is denied on
    /// directory handles even when READ_CONTROL|WRITE_DAC was requested, so the
    /// caller opens the directory with the needed rights up front and this
    /// variant skips reopening entirely.
    #[cfg(test)]
    pub(super) fn set_test_dacl_handle(file: &File, sddl: &str) -> io::Result<()> {
        use windows_sys::Win32::Security::Authorization::SetSecurityInfo;
        use windows_sys::Win32::Security::PROTECTED_DACL_SECURITY_INFORMATION;
        let security = descriptor(sddl)?;
        let mut present = 0;
        let mut defaulted = 0;
        let mut acl = null_mut();
        // SAFETY: parsed descriptor is valid and alive; all outputs are writable.
        if unsafe { GetSecurityDescriptorDacl(security.0, &mut present, &mut acl, &mut defaulted) }
            == 0
            || present == 0
            || acl.is_null()
        {
            return Err(denied("test DACL missing"));
        }
        // SAFETY: the caller opened the handle with READ_CONTROL|WRITE_DAC; the
        // ACL is valid and alive. Tests change only their own files/directories;
        // no SACL, owner or privilege changes.
        let result = unsafe {
            SetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null_mut(),
            )
        };
        if result != ERROR_SUCCESS {
            return Err(io::Error::from_raw_os_error(result as i32));
        }
        Ok(())
    }
}

// ─── load_inventory ───────────────────────────────────────────────────────────

/// Reads and deserialises an [`crate::items::inventory::Inventory`] from the
/// JSON file at `path`.
///
/// Zero counts and `item_byte` values decoded as `I::default()` or `is_empty()`
/// become empty slots at their original indexes. Live duplicate stacks are not
/// merged. Capacity must equal the slot vector length, and the total of each
/// item (by equality) must fit in `u32`.
///
/// # Parameters
///
/// * `path` — filesystem path of the JSON file written by [`save_inventory`].
///
/// # Errors
///
/// Returns a boxed error if:
/// * Capacity/layout or item totals are invalid ([`std::io::ErrorKind::InvalidData`]).
/// * The file does not exist or cannot be read ([`std::io::Error`]).
/// * The file contents are not valid JSON or do not match the expected schema
///   ([`serde_json::Error`]).
///
/// # Examples
///
/// ```rust,no_run
/// use voxel_framework::items::{DefaultItem, load_inventory};
///
/// let inv = load_inventory::<DefaultItem>("saves/player.json")
///     .expect("failed to load inventory");
/// println!("iron ore: {}", inv.count_of(DefaultItem::IronOre));
/// ```
pub fn load_inventory<I: ItemType + for<'de> Deserialize<'de>>(
    path: &str,
) -> Result<Inventory<I>, Box<dyn std::error::Error>> {
    let json = fs::read_to_string(load_target(Path::new(path))?)?;
    let dto: InventoryDto = serde_json::from_str(&json)?;
    Ok(decode_inventory::<I>(&dto, io::ErrorKind::InvalidData)?)
}

/// Path `load_inventory` reads, following a final-component symlink with the
/// same manual resolution [`prepare_target`] uses. Plain `fs::read_to_string`
/// follows such a link in-kernel, which fails with `ERROR_INVALID_NAME` (123)
/// on CI runners whose `%TEMP%` mixes the 8.3 short name (`RUNNER~1`) with
/// long names when the link target is relative (`..`); reading the manually
/// resolved path keeps dangling/nonregular semantics (`read_to_string` still
/// surfaces them) without opening through the reparse point.
fn load_target(path: &Path) -> io::Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => resolved_link_target(path),
        Ok(_) => Ok(path.to_path_buf()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(error) => Err(error),
    }
}

fn decode_inventory<I: ItemType>(
    dto: &InventoryDto,
    error_kind: io::ErrorKind,
) -> io::Result<Inventory<I>> {
    if dto.capacity != dto.slots.len() {
        return Err(io::Error::new(
            error_kind,
            "inventory capacity does not match slot count",
        ));
    }

    let slots = dto
        .slots
        .iter()
        .map(|slot| {
            slot.as_ref().and_then(|s| {
                let item = I::from_byte(s.item_byte);
                (s.count > 0 && item != I::default() && !item.is_empty()).then_some(InventorySlot {
                    item,
                    count: s.count,
                })
            })
        })
        .collect();

    let inventory = Inventory {
        capacity: dto.capacity,
        slots,
    };
    inventory
        .validate_integrity()
        .map_err(|_| io::Error::new(error_kind, "invalid inventory item totals"))?;
    Ok(inventory)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::DefaultItem;

    fn sample(count: u32) -> Inventory<DefaultItem> {
        Inventory {
            capacity: 5,
            slots: vec![
                None,
                Some(InventorySlot {
                    item: DefaultItem::IronOre,
                    count,
                }),
                None,
                Some(InventorySlot {
                    item: DefaultItem::Crystal,
                    count: 3,
                }),
                Some(InventorySlot {
                    item: DefaultItem::IronOre,
                    count: 2,
                }),
            ],
        }
    }

    fn assert_layout<I: ItemType>(actual: &Inventory<I>, expected: &Inventory<I>) {
        assert_eq!(actual.capacity, expected.capacity);
        assert_eq!(actual.slots.len(), expected.slots.len());
        for (actual, expected) in actual.slots.iter().zip(&expected.slots) {
            match (actual, expected) {
                (None, None) => {}
                (Some(actual), Some(expected)) => {
                    assert!(actual.item == expected.item);
                    assert_eq!(actual.count, expected.count);
                }
                _ => panic!("slot layout changed"),
            }
        }
    }

    fn io_kind(error: &(dyn std::error::Error + 'static)) -> io::ErrorKind {
        error.downcast_ref::<io::Error>().expect("I/O error").kind()
    }

    fn entries(directory: &Path) -> Vec<PathBuf> {
        let mut entries: Vec<_> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        entries
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("inventory.json");
        let inventory = sample(7);
        save_inventory(&inventory, path.to_str().unwrap()).unwrap();
        let loaded = load_inventory::<DefaultItem>(path.to_str().unwrap()).unwrap();
        assert_layout(&loaded, &inventory);
        let json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(json.as_object().unwrap().len(), 2);
        assert_eq!(json["capacity"], 5);
        assert_eq!(
            json["slots"][1]["item_byte"],
            DefaultItem::IronOre.to_byte()
        );
        assert_eq!(json["slots"][1]["count"], 7);
        assert_eq!(json["slots"][1].as_object().unwrap().len(), 2);
        assert_eq!(entries(directory.path()), vec![path]);
    }

    #[test]
    fn zero_capacity_roundtrip_and_nested_parent_creation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/saves/empty.json");
        let inventory = Inventory::<DefaultItem>::new(0);
        save_inventory(&inventory, path.to_str().unwrap()).unwrap();
        assert_layout(
            &load_inventory::<DefaultItem>(path.to_str().unwrap()).unwrap(),
            &inventory,
        );
    }

    #[test]
    fn bare_basename_save_without_changing_current_directory() {
        // TempPath closes the initial handle (important on Windows), owns cleanup,
        // and gives this test a unique basename without changing process-wide cwd.
        let guard = NamedTempFile::new_in(".").unwrap().into_temp_path();
        let basename = guard.file_name().unwrap().to_str().unwrap();
        assert_eq!(parent_directory(Path::new(basename)), Path::new("."));
        let inventory = sample(4);
        save_inventory(&inventory, basename).unwrap();
        assert_layout(
            &load_inventory::<DefaultItem>(basename).unwrap(),
            &inventory,
        );
    }

    #[test]
    fn invalid_source_preserves_old_file_or_missing_destination() {
        let directory = tempfile::tempdir().unwrap();
        let old = directory.path().join("old.json");
        let absent = directory.path().join("missing/absent.json");
        fs::write(&old, b"old bytes").unwrap();
        let mut mismatch = sample(1);
        mismatch.capacity += 1;
        let mut zero = sample(1);
        zero.slots[1].as_mut().unwrap().count = 0;
        let mut sentinel = sample(1);
        sentinel.slots[1].as_mut().unwrap().item = DefaultItem::Empty;
        let overflow = sample(u32::MAX);
        for inventory in [mismatch, zero, sentinel, overflow] {
            for target in [&old, &absent] {
                let error = save_inventory(&inventory, target.to_str().unwrap()).unwrap_err();
                assert_eq!(io_kind(error.as_ref()), io::ErrorKind::InvalidInput);
                assert_eq!(fs::read(&old).unwrap(), b"old bytes");
                assert!(!absent.parent().unwrap().exists());
                assert_eq!(entries(directory.path()), vec![old.clone()]);
            }
        }
    }

    fn aliased_inventory(count: u32) -> Inventory<DefaultItem> {
        Inventory {
            capacity: 3,
            slots: vec![
                Some(InventorySlot {
                    item: DefaultItem::IronOre,
                    count,
                }),
                None,
                Some(InventorySlot {
                    item: DefaultItem::IronOreItem,
                    count: 1,
                }),
            ],
        }
    }

    #[test]
    fn save_valid_byte_aliases_loads_collapsed_items_at_original_indexes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("inventory.json");
        let inventory = aliased_inventory(u32::MAX - 1);
        assert_ne!(DefaultItem::IronOre, DefaultItem::IronOreItem);
        assert_eq!(
            DefaultItem::IronOre.to_byte(),
            DefaultItem::IronOreItem.to_byte()
        );
        assert!(inventory.validate_integrity().is_ok());
        save_inventory(&inventory, path.to_str().unwrap()).unwrap();
        let loaded = load_inventory::<DefaultItem>(path.to_str().unwrap()).unwrap();
        let mut expected = inventory;
        expected.slots[2].as_mut().unwrap().item = DefaultItem::IronOre;
        assert_layout(&loaded, &expected);
        assert_eq!(loaded.count_of(DefaultItem::IronOre), u32::MAX);
        assert_eq!(entries(directory.path()), vec![path]);
    }

    #[test]
    fn save_rejects_byte_alias_overflow_before_filesystem_changes() {
        let directory = tempfile::tempdir().unwrap();
        let old = directory.path().join("old.json");
        let absent = directory.path().join("missing/absent.json");
        let original = sample(7);
        save_inventory(&original, old.to_str().unwrap()).unwrap();
        let old_bytes = fs::read(&old).unwrap();
        let inventory = aliased_inventory(u32::MAX);
        // Runtime equality considers these different items; only decoded totals overflow.
        assert!(inventory.validate_integrity().is_ok());
        for target in [&old, &absent] {
            let error = save_inventory(&inventory, target.to_str().unwrap()).unwrap_err();
            assert_eq!(io_kind(error.as_ref()), io::ErrorKind::InvalidInput);
            assert_eq!(fs::read(&old).unwrap(), old_bytes);
            assert_layout(
                &load_inventory::<DefaultItem>(old.to_str().unwrap()).unwrap(),
                &original,
            );
            assert!(!absent.parent().unwrap().exists());
            assert_eq!(entries(directory.path()), vec![old.clone()]);
        }
    }

    #[test]
    fn load_rejects_missing_malformed_schema_shape_and_overflow() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("inventory.json");
        let error = load_inventory::<DefaultItem>(path.to_str().unwrap()).unwrap_err();
        assert_eq!(io_kind(error.as_ref()), io::ErrorKind::NotFound);
        for json in ["{", "{}", r#"{"capacity":0,"slots":[{"item_byte":11}]}"#] {
            fs::write(&path, json).unwrap();
            let error = load_inventory::<DefaultItem>(path.to_str().unwrap()).unwrap_err();
            assert!(error.downcast_ref::<serde_json::Error>().is_some());
        }
        for json in [
            r#"{"capacity":2,"slots":[null]}"#,
            r#"{"capacity":0,"slots":[null]}"#,
            r#"{"capacity":2,"slots":[{"item_byte":11,"count":4294967295},{"item_byte":11,"count":1}]}"#,
        ] {
            fs::write(&path, json).unwrap();
            let error = load_inventory::<DefaultItem>(path.to_str().unwrap()).unwrap_err();
            assert_eq!(io_kind(error.as_ref()), io::ErrorKind::InvalidData);
        }
    }

    #[test]
    fn load_normalizes_zero_unknown_and_default_without_repacking() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("inventory.json");
        fs::write(
            &path,
            r#"{"capacity":7,"slots":[null,{"item_byte":11,"count":0},{"item_byte":255,"count":99},{"item_byte":0,"count":2},{"item_byte":11,"count":4294967294},null,{"item_byte":11,"count":1}]}"#,
        ).unwrap();
        let inventory = load_inventory::<DefaultItem>(path.to_str().unwrap()).unwrap();
        assert_eq!(inventory.capacity, 7);
        assert!(inventory.slots[..4].iter().all(Option::is_none));
        assert_eq!(inventory.slots[4].as_ref().unwrap().count, u32::MAX - 1);
        assert!(inventory.slots[5].is_none());
        assert_eq!(inventory.slots[6].as_ref().unwrap().count, 1);
        assert_eq!(inventory.count_of(DefaultItem::IronOre), u32::MAX);
    }

    #[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Debug)]
    enum TestItem {
        #[default]
        DefaultSentinel,
        EmptyAlias,
        Live,
    }

    impl ItemType for TestItem {
        fn name(&self) -> &'static str {
            "test"
        }
        fn color(&self) -> [f32; 4] {
            [1.0; 4]
        }
        fn to_byte(&self) -> u8 {
            *self as u8
        }
        fn from_byte(byte: u8) -> Self {
            match byte {
                1 => Self::EmptyAlias,
                2 => Self::Live,
                _ => Self::DefaultSentinel,
            }
        }
        fn is_empty(&self) -> bool {
            *self == Self::EmptyAlias
        }
    }

    #[test]
    fn load_checks_default_equality_and_nondefault_empty_independently() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("inventory.json");
        fs::write(&path, r#"{"capacity":3,"slots":[{"item_byte":0,"count":1},{"item_byte":1,"count":1},{"item_byte":2,"count":1}]}"#).unwrap();
        let inventory = load_inventory::<TestItem>(path.to_str().unwrap()).unwrap();
        assert!(inventory.slots[0].is_none());
        assert!(inventory.slots[1].is_none());
        assert_eq!(inventory.slots[2].as_ref().unwrap().item, TestItem::Live);
        let invalid = Inventory {
            capacity: 1,
            slots: vec![Some(InventorySlot {
                item: TestItem::EmptyAlias,
                count: 1,
            })],
        };
        let old = fs::read(&path).unwrap();
        let error = save_inventory(&invalid, path.to_str().unwrap()).unwrap_err();
        assert_eq!(io_kind(error.as_ref()), io::ErrorKind::InvalidInput);
        assert_eq!(fs::read(path).unwrap(), old);
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Failure {
        PrefixWrite,
        Sync,
        Replace,
        RealReplace,
    }

    struct InjectedOps {
        failure: Option<Failure>,
        events: Vec<&'static str>,
        expected_parent: PathBuf,
        expected_permissions: Permissions,
    }

    impl AtomicWriteOps for InjectedOps {
        fn write_all(&mut self, file: &mut File, bytes: &[u8]) -> io::Result<()> {
            self.events.push("write");
            if self.failure == Some(Failure::PrefixWrite) {
                file.write_all(&bytes[..bytes.len().min(13)])?;
                return Err(io::Error::other("injected prefix-write failure"));
            }
            FileWriteOps.write_all(file, bytes)
        }

        fn sync_all(&mut self, file: &File) -> io::Result<()> {
            self.events.push("sync");
            let actual = file.metadata()?.permissions();
            assert_eq!(actual.readonly(), self.expected_permissions.readonly());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(actual.mode(), self.expected_permissions.mode());
            }
            if self.failure == Some(Failure::Sync) {
                return Err(io::Error::other("injected sync failure"));
            }
            FileWriteOps.sync_all(file)
        }

        fn replace(&mut self, temp: NamedTempFile, target: &Path) -> io::Result<()> {
            self.events.push("replace");
            assert_eq!(
                fs::canonicalize(parent_directory(temp.path()))?,
                self.expected_parent
            );
            if self.failure == Some(Failure::Replace) {
                return Err(io::Error::other("injected replacement failure"));
            }
            if self.failure == Some(Failure::RealReplace) {
                // Exercise the production PersistError cleanup, without modifying
                // the real destination or depending on platform permission rules.
                return FileWriteOps.replace(temp, &target.join("missing/invalid.json"));
            }
            FileWriteOps.replace(temp, target)
        }
    }

    #[test]
    fn injected_failures_preserve_existing_and_absent_targets_and_clean_temps() {
        for existing in [false, true] {
            for failure in [
                Failure::PrefixWrite,
                Failure::Sync,
                Failure::Replace,
                Failure::RealReplace,
            ] {
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("inventory.json");
                let expected_permissions = if existing {
                    fs::write(&path, b"old bytes").unwrap();
                    fs::metadata(&path).unwrap().permissions()
                } else {
                    let probe = NamedTempFile::new_in(directory.path()).unwrap();
                    probe.as_file().metadata().unwrap().permissions()
                };
                let before = entries(directory.path());
                let mut ops = InjectedOps {
                    failure: Some(failure),
                    events: vec![],
                    expected_parent: fs::canonicalize(directory.path()).unwrap(),
                    expected_permissions,
                };
                assert!(save_inventory_with_ops(&sample(7), &path, &mut ops).is_err());
                let expected_events = match failure {
                    Failure::PrefixWrite => vec!["write"],
                    Failure::Sync => vec!["write", "sync"],
                    Failure::Replace | Failure::RealReplace => vec!["write", "sync", "replace"],
                };
                assert_eq!(ops.events, expected_events);
                if existing {
                    assert_eq!(fs::read(&path).unwrap(), b"old bytes");
                } else {
                    assert!(!path.exists());
                }
                assert_eq!(entries(directory.path()), before);
            }
        }
    }

    #[test]
    fn successful_replacement_orders_write_permissions_sync_then_replace() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("inventory.json");
        fs::write(&path, b"old").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, Permissions::from_mode(0o640)).unwrap();
        }
        let permissions = fs::metadata(&path).unwrap().permissions();
        let mut ops = InjectedOps {
            failure: None,
            events: vec![],
            expected_parent: fs::canonicalize(directory.path()).unwrap(),
            expected_permissions: permissions.clone(),
        };
        let inventory = sample(8);
        save_inventory_with_ops(&inventory, &path, &mut ops).unwrap();
        assert_eq!(ops.events, vec!["write", "sync", "replace"]);
        assert_layout(
            &load_inventory::<DefaultItem>(path.to_str().unwrap()).unwrap(),
            &inventory,
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().readonly(),
            permissions.readonly()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode(),
                permissions.mode()
            );
        }
        assert_eq!(entries(directory.path()), vec![path]);
    }

    #[test]
    fn read_only_and_directory_targets_are_rejected_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("readonly.json");
        fs::write(&path, b"old").unwrap();
        let original = fs::metadata(&path).unwrap().permissions();
        let mut readonly = original.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&path, readonly).unwrap();
        let result = save_inventory(&sample(2), path.to_str().unwrap());
        let bytes = fs::read(&path).unwrap();
        // Restore before assertions so tempdir cleanup also works on Windows.
        fs::set_permissions(&path, original).unwrap();
        assert_eq!(
            io_kind(result.unwrap_err().as_ref()),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(bytes, b"old");
        let result = save_inventory(&sample(2), directory.path().to_str().unwrap());
        assert_eq!(
            io_kind(result.unwrap_err().as_ref()),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(entries(directory.path()), vec![path]);
    }

    #[cfg(unix)]
    #[test]
    fn new_save_has_private_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("new.json");
        save_inventory(&sample(1), path.to_str().unwrap()).unwrap();
        assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o077, 0);
    }

    #[cfg(windows)]
    #[test]
    fn windows_private_creation_excludes_broad_parent_default_inheritance() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, READ_CONTROL, WRITE_DAC,
        };
        let directory = tempfile::tempdir().unwrap();
        // SetSecurityInfo needs WRITE_DAC on the handle it applies, so the open
        // grants READ_CONTROL|WRITE_DAC up front; the handle variant never
        // reopens (ReOpenFile grants only rights the original open allowed).
        let parent = OpenOptions::new()
            .read(true)
            .access_mode(READ_CONTROL | WRITE_DAC)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(directory.path())
            .unwrap();
        // Explicit Everyone/full-control inheritance, confined to this tempdir.
        windows_private::set_test_dacl_handle(&parent, "D:P(A;OICI;FA;;;WD)").unwrap();
        drop(parent);
        let inherited = NamedTempFile::new_in(directory.path()).unwrap();
        assert!(windows_private::verify_private(inherited.as_file()).is_err());
        drop(inherited);
        let private = private_tempfile(directory.path()).unwrap();
        assert_eq!(private.as_file().metadata().unwrap().len(), 0);
        windows_private::verify_private(private.as_file()).unwrap();
        drop(private);
        let path = directory.path().join("inventory.json");
        save_inventory(&sample(7), path.to_str().unwrap()).unwrap();
        windows_private::verify_private(&File::open(&path).unwrap()).unwrap();
        assert_layout(
            &load_inventory::<DefaultItem>(path.to_str().unwrap()).unwrap(),
            &sample(7),
        );
        assert_eq!(entries(directory.path()), vec![path]);
    }

    #[cfg(windows)]
    struct WindowsSecurityOps {
        corrupt_dacl: bool,
        unsupported_volume: bool,
        events: Vec<&'static str>,
    }

    #[cfg(windows)]
    impl AtomicWriteOps for WindowsSecurityOps {
        fn check_private(&mut self, file: &File) -> io::Result<()> {
            self.events.push("private");
            assert_eq!(file.metadata()?.len(), 0);
            // Assert that production creation itself is private, not repaired
            // by the prewrite check. Then inject a genuine insecure file DACL.
            FileWriteOps.check_private(file)?;
            if self.corrupt_dacl {
                windows_private::set_test_dacl(file, "D:P(A;;FA;;;WD)")?;
            }
            if self.unsupported_volume {
                return windows_private::reject_unsupported_volume();
            }
            FileWriteOps.check_private(file)
        }
        fn write_all(&mut self, file: &mut File, bytes: &[u8]) -> io::Result<()> {
            self.events.push("write");
            // A second real check proves the write boundary has a private DACL.
            windows_private::verify_private(file)?;
            FileWriteOps.write_all(file, bytes)
        }
        fn sync_all(&mut self, file: &File) -> io::Result<()> {
            self.events.push("sync");
            FileWriteOps.sync_all(file)
        }
        fn replace(&mut self, temp: NamedTempFile, target: &Path) -> io::Result<()> {
            self.events.push("replace");
            FileWriteOps.replace(temp, target)
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_security_check_precedes_json_write_and_final_file_is_private() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("inventory.json");
        let mut ops = WindowsSecurityOps {
            corrupt_dacl: false,
            unsupported_volume: false,
            events: vec![],
        };
        save_inventory_with_ops(&sample(7), &path, &mut ops).unwrap();
        assert_eq!(ops.events, vec!["private", "write", "sync", "replace"]);
        windows_private::verify_private(&File::open(&path).unwrap()).unwrap();
        assert_eq!(entries(directory.path()), vec![path]);
    }

    #[cfg(windows)]
    #[test]
    fn windows_security_failures_preserve_old_or_absent_save_without_writes_or_temps() {
        for existing in [false, true] {
            for corrupt_dacl in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("inventory.json");
                if existing {
                    save_inventory(&sample(3), path.to_str().unwrap()).unwrap();
                }
                let old = existing.then(|| fs::read(&path).unwrap());
                let before = entries(directory.path());
                let mut ops = WindowsSecurityOps {
                    corrupt_dacl,
                    unsupported_volume: !corrupt_dacl,
                    events: vec![],
                };
                let error = save_inventory_with_ops(&sample(7), &path, &mut ops).unwrap_err();
                assert_eq!(io_kind(error.as_ref()), io::ErrorKind::PermissionDenied);
                assert_eq!(ops.events, vec!["private"]);
                if let Some(bytes) = old {
                    assert_eq!(fs::read(&path).unwrap(), bytes);
                    assert_layout(
                        &load_inventory::<DefaultItem>(path.to_str().unwrap()).unwrap(),
                        &sample(3),
                    );
                } else {
                    assert!(!path.exists());
                }
                assert_eq!(entries(directory.path()), before);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_socket_and_hardlinks_are_rejected_unchanged() {
        use std::os::unix::net::UnixListener;
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("socket");
        match UnixListener::bind(&socket) {
            Ok(_listener) => {
                let error = save_inventory(&sample(1), socket.to_str().unwrap()).unwrap_err();
                assert_eq!(io_kind(error.as_ref()), io::ErrorKind::InvalidInput);
            }
            // Sandboxed CI environments can deny AF_UNIX socket creation before the
            // persistence code runs. Keep the hard-link coverage below regardless.
            Err(error)
                if error.kind() == io::ErrorKind::PermissionDenied
                    || matches!(error.raw_os_error(), Some(1 | 13)) =>
            {
                eprintln!(
                    "skipping Unix-socket rejection assertion: environment denied socket creation: {error}"
                );
            }
            Err(error) => panic!("Unix socket creation failed unexpectedly: {error}"),
        }
        let path = directory.path().join("inventory.json");
        let alias = directory.path().join("alias.json");
        fs::write(&path, b"old").unwrap();
        fs::hard_link(&path, &alias).unwrap();
        let before = entries(directory.path());
        for target in [&path, &alias] {
            let error = save_inventory(&sample(1), target.to_str().unwrap()).unwrap_err();
            assert_eq!(io_kind(error.as_ref()), io::ErrorKind::InvalidInput);
            assert_eq!(fs::read(&path).unwrap(), b"old");
            assert_eq!(fs::read(&alias).unwrap(), b"old");
            assert_eq!(entries(directory.path()), before);
        }
    }

    #[cfg(any(unix, windows))]
    fn symlink_file(target: &Path, link: &Path) -> io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(target, link)
        }
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn final_symlink_is_preserved_and_temp_is_in_target_directory() {
        let directory = tempfile::tempdir().unwrap();
        let targets = directory.path().join("targets");
        let links = directory.path().join("links");
        fs::create_dir(&targets).unwrap();
        fs::create_dir(&links).unwrap();
        let target = targets.join("inventory.json");
        let link = links.join("save.json");
        fs::write(&target, b"old").unwrap();
        let relative_target = Path::new("../targets/inventory.json");
        if let Err(error) = symlink_file(relative_target, &link) {
            #[cfg(windows)]
            if error.kind() == io::ErrorKind::PermissionDenied {
                eprintln!(
                    "symlink test requires Windows Developer Mode or symlink privilege: {error}"
                );
                return;
            }
            panic!("symlink creation failed: {error}");
        }
        let mut ops = InjectedOps {
            failure: None,
            events: vec![],
            expected_parent: fs::canonicalize(&targets).unwrap(),
            expected_permissions: fs::metadata(&target).unwrap().permissions(),
        };
        for failure in [Failure::PrefixWrite, Failure::Sync, Failure::Replace] {
            ops.failure = Some(failure);
            ops.events.clear();
            assert!(save_inventory_with_ops(&sample(9), &link, &mut ops).is_err());
            assert_eq!(fs::read(&target).unwrap(), b"old");
            assert_eq!(fs::read_link(&link).unwrap(), relative_target);
            assert_eq!(entries(&targets), vec![target.clone()]);
            assert_eq!(entries(&links), vec![link.clone()]);
        }
        ops.failure = None;
        ops.events.clear();
        let inventory = sample(9);
        save_inventory_with_ops(&inventory, &link, &mut ops).unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&link).unwrap(), relative_target);
        assert_layout(
            &load_inventory::<DefaultItem>(link.to_str().unwrap()).unwrap(),
            &inventory,
        );
        assert_eq!(entries(&targets), vec![target]);
        assert_eq!(entries(&links), vec![link]);
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn dangling_symlink_is_not_replaced_or_followed_into_new_file() {
        let directory = tempfile::tempdir().unwrap();
        let link = directory.path().join("dangling.json");
        let missing = directory.path().join("missing.json");
        if let Err(error) = symlink_file(Path::new("missing.json"), &link) {
            #[cfg(windows)]
            if error.kind() == io::ErrorKind::PermissionDenied {
                eprintln!(
                    "symlink test requires Windows Developer Mode or symlink privilege: {error}"
                );
                return;
            }
            panic!("symlink creation failed: {error}");
        }
        assert!(save_inventory(&sample(1), link.to_str().unwrap()).is_err());
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&link).unwrap(), Path::new("missing.json"));
        assert!(!missing.exists());
        assert_eq!(entries(directory.path()), vec![link]);
    }

    #[test]
    fn concurrent_writers_leave_one_whole_snapshot_and_no_temps() {
        use std::sync::{Arc, Barrier};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("inventory.json");
        save_inventory(&sample(1), path.to_str().unwrap()).unwrap();
        let barrier = Arc::new(Barrier::new(8));
        let writers: Vec<_> = (2..10)
            .map(|count| {
                let path = path.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    barrier.wait();
                    save_inventory(&sample(count), path.to_str().unwrap()).unwrap();
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let loaded = load_inventory::<DefaultItem>(path.to_str().unwrap()).unwrap();
        let count = loaded.slots[1].as_ref().unwrap().count;
        assert!((2..10).contains(&count));
        assert_layout(&loaded, &sample(count));
        assert_eq!(entries(directory.path()), vec![path]);
    }

    #[cfg(windows)]
    #[test]
    fn lexically_normalize_windows_collapses_dots_and_parents() {
        let cases = [
            // The CI shape: 8.3 short name + long dirs + relative `..` link target.
            (
                r"C:\Users\RUNNER~1\AppData\Local\Temp\.tmpX\links\..\targets\inv.json",
                r"C:\Users\RUNNER~1\AppData\Local\Temp\.tmpX\targets\inv.json",
            ),
            (
                r"C:\Users\app\.\links\inv.json",
                r"C:\Users\app\links\inv.json",
            ),
            (r"C:\a\b\..\..\c", r"C:\c"),
            (r"C:\a\..\..\..", r"C:\"),
            // Root-relative and plain relative paths stay clean.
            (r"\a\b\..\c", r"\a\c"),
            (r"a\b\..\..\..\c", r"..\c"),
            (r"..\a", r"..\a"),
            // Verbatim prefixes pass through untouched.
            (r"\\?\C:\a\..\b", r"\\?\C:\b"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                lexically_normalize_windows(Path::new(input)),
                Path::new(expected),
                "normalizing {input}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn lexically_normalize_windows_preserves_drive_and_unc_prefix_case() {
        let drive = lexically_normalize_windows(Path::new(r"c:\Temp\a\..\b.json"));
        assert_eq!(drive, Path::new(r"c:\Temp\b.json"));
        let unc = lexically_normalize_windows(Path::new(r"\\SERVER\Share\a\..\b.json"));
        assert_eq!(unc, Path::new(r"\\SERVER\Share\b.json"));
    }
}
