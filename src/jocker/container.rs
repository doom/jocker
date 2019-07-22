use std::ffi::CString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use failure::{format_err, Error, Fail, ResultExt};
use flate2::write::GzEncoder;
use flate2::Compression;
use nix::mount::{mount, umount, umount2, MntFlags, MsFlags};
use nix::sched::{clone, CloneFlags};
use nix::sys::signal::SIGCHLD;
use nix::sys::stat::{fchmodat, makedev, mknod, FchmodatFlags, Mode, SFlag};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{chdir, execv, getpid, pivot_root, sethostname};
use serde_derive::{Deserialize, Serialize};

use super::image::{ExtractedImage, ImageError};
use super::Config;
use crate::jocker::image::Image;

/// Error type for container-related errors
#[derive(Fail, Debug)]
pub enum ContainerError {
    /// The container has an invalid configuration file
    #[fail(display = "invalid configuration file")]
    InvalidConfigurationFile,

    /// The container was executed with an invalid command
    #[fail(display = "invalid command")]
    InvalidCommand,

    /// The given directory could not be used as a container directory
    #[fail(display = "invalid container directory")]
    InvalidContainerDirectory(std::io::Error),

    /// The container's configuration file could not be opened
    #[fail(display = "cannot open the configuration file: {}", _0)]
    CannotOpenConfigurationFile(std::io::Error),

    /// The container's configuration file could not be saved
    #[fail(display = "cannot save the configuration file")]
    CannotSaveConfigurationFile,

    /// The container could not be created
    #[fail(display = "cannot create the container: {}", _0)]
    CreationError(std::io::Error),

    /// The container could not be initialized with an image
    #[fail(display = "cannot initialize the container: {}", _0)]
    InitializationError(ImageError),

    /// The container could not be exported as an image
    #[fail(display = "cannot export the container as an image: {}", _0)]
    ExportError(ImageError),

    /// The container could not be archived for export
    #[fail(display = "cannot archive the container: {}", _0)]
    ArchiveError(std::io::Error),

    /// The container could not be executed successfully
    #[fail(display = "container execution failed: {}", _0)]
    ContainerExecutionError(nix::Error),

    /// The container could not be configured successfully
    #[fail(display = "unable to setup the container")]
    ContainerSetupError,

    /// The command executed in the container exited with an error code
    #[fail(display = "command exited with error code: {}", _0)]
    CommandExitedWithError(i32),

    /// The container exited abnormally
    #[fail(display = "the container exited abnormally")]
    ContainerExitedAbnormally,
}

/// Structure describing the configuration of a container
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContainerConfig {
    name: String,
    image_name: String,
}

impl ContainerConfig {
    fn from(name: String, image_name: String) -> Self {
        Self { name, image_name }
    }

    /// Load a configuration from a file
    pub fn load_from_file(path: &Path) -> Result<Self, ContainerError> {
        let file = fs::File::open(path).map_err(ContainerError::CannotOpenConfigurationFile)?;

        Ok(serde_json::from_reader(&file).map_err(|_| ContainerError::InvalidConfigurationFile)?)
    }

    /// Save the configuration to a file
    pub fn save(&self, path: &Path) -> Result<(), ContainerError> {
        let file =
            fs::File::create(path).map_err(|_| ContainerError::CannotSaveConfigurationFile)?;

        Ok(serde_json::to_writer(file, self)
            .map_err(|_| ContainerError::CannotSaveConfigurationFile)?)
    }

    /// Retrieve the name of the container
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Retrieve the name of the container's image
    pub fn image_name(&self) -> &str {
        &self.image_name
    }
}

/// Structure representing a container
#[derive(Debug)]
pub struct Container {
    config: ContainerConfig,
    path: PathBuf,
}

impl Container {
    /// Load a container from a directory
    fn from_directory(path: PathBuf) -> Result<Self, ContainerError> {
        let config = ContainerConfig::load_from_file(&path.join("config.json"))?;

        Ok(Self { config, path })
    }

    /// Create a container from a directory containing an initialized container
    pub fn create(name: String, path: PathBuf, image_name: String) -> Result<Self, ContainerError> {
        fs::create_dir_all(&path).map_err(ContainerError::CreationError)?;
        let config = ContainerConfig::from(name, image_name);

        config.save(&path.join("config.json"))?;

        Ok(Self { config, path })
    }

    /// Retrieve the name of the container
    pub fn name(&self) -> &str {
        self.config.name()
    }

    /// Retrieve the path to the container's directory
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn setup_overlay(&self, image: &ExtractedImage) -> Result<(), ContainerError> {
        // Create the "upper directory" for the overlay filesystem
        let upper_dir_path = self.path.join("cow_rw");
        if !upper_dir_path.exists() {
            fs::create_dir(&upper_dir_path).map_err(ContainerError::CreationError)?;
        }

        // Create the "working directory" for the overlay filesystem
        // Quoting the specification for OverlayFS, it is "used to prepare files before
        // they are switched to the overlay destination in an atomic action"
        let work_dir_path = self.path.join("cow_workdir");
        if !work_dir_path.exists() {
            fs::create_dir(&work_dir_path).map_err(ContainerError::CreationError)?;
        }

        // Create the target directory for the overlay filesystem
        let rootfs_path = self.path.join("rootfs");
        if !rootfs_path.exists() {
            fs::create_dir(&rootfs_path).map_err(ContainerError::CreationError)?;
        }

        mount(
            Some(Path::new("overlay")),
            &rootfs_path,
            Some(Path::new("overlay")),
            MsFlags::MS_SILENT,
            Some(Path::new(&format!(
                "lowerdir={},upperdir={},workdir={}",
                image.path().display(),
                upper_dir_path.display(),
                work_dir_path.display(),
            ))),
        )
        .map_err(|_| ContainerError::ContainerSetupError)
    }

    fn mount_kernel_filesystems(&self) -> Result<(), nix::Error> {
        let mounts = [
            (
                None: Option<&Path>,
                PathBuf::from("proc"),
                Some("proc"),
                MsFlags::MS_NOATIME,
                None,
            ),
            (
                None,
                PathBuf::from("sys"),
                Some("sysfs"),
                MsFlags::MS_NOATIME,
                None,
            ),
            (
                None,
                PathBuf::from("tmp"),
                Some("tmpfs"),
                MsFlags::MS_NOSUID | MsFlags::MS_STRICTATIME,
                None,
            ),
            (
                None,
                PathBuf::from("dev").join("pts"),
                Some("devpts"),
                MsFlags::MS_NOATIME,
                None,
            ),
        ];

        for (source, target, fstype, flags, data) in mounts.iter() {
            mount::<Path, Path, Path, Path>(
                *source,
                &self.path.join("rootfs").join(target),
                fstype.map(Path::new),
                *flags,
                *data,
            )?;
        }

        Ok(())
    }

    fn create_devices(&self) -> Result<(), nix::Error> {
        let dev_path = self.path.join("rootfs").join("dev");
        let rw_all = Mode::S_IRUSR
            | Mode::S_IWUSR
            | Mode::S_IRGRP
            | Mode::S_IWGRP
            | Mode::S_IROTH
            | Mode::S_IWOTH;
        let devices = [
            ("null", SFlag::S_IFCHR, (1, 3)),
            ("zero", SFlag::S_IFCHR, (1, 5)),
            ("random", SFlag::S_IFCHR, (1, 8)),
            ("urandom", SFlag::S_IFCHR, (1, 9)),
        ];

        for (name, kind, (major, minor)) in &devices {
            let path = dev_path.join(name);

            if !path.exists() {
                mknod(&path, *kind, rw_all, makedev(*major, *minor))?;
                // Ensure the file's permissions are as expected (the umask could have restricted them)
                fchmodat(None, &path, rw_all, FchmodatFlags::FollowSymlink)?;
            }
        }

        Ok(())
    }

    fn move_to_new_root(&self) -> Result<(), Error> {
        let old_root = self.path.join("rootfs").join("old_root");

        fs::create_dir(&old_root).map_err(|_| nix::Error::last())?;
        pivot_root(&self.path.join("rootfs"), &old_root)?;
        chdir(Path::new("/"))?;

        Ok(())
    }

    fn setup_cgroup(&self, group_name: &str) -> Result<(), Error> {
        let jocker_cgroup_cpu_path = Path::new("/sys/fs/cgroup").join(group_name).join("jocker");
        let container_cgroup_cpu_path = jocker_cgroup_cpu_path.join(&self.config.name());

        if !container_cgroup_cpu_path.exists() {
            fs::create_dir_all(&container_cgroup_cpu_path)?;
        }

        let mut tasks_file = fs::File::create(container_cgroup_cpu_path.join("tasks"))?;
        tasks_file.write_fmt(format_args!("{}", getpid()))?;

        Ok(())
    }

    fn setup_memory_cgroup(&self) -> Result<(), Error> {
        self.setup_cgroup("memory")?;

        // TODO: memory limit, swap size, swappiness
        Ok(())
    }

    fn setup_cpu_cgroup(&self) -> Result<(), Error> {
        self.setup_cgroup("cpu")?;

        // TODO: CPU shares, CPU number, allowed CPUs
        Ok(())
    }

    fn extract_image(&self, config: &Config) -> Result<ExtractedImage, ContainerError> {
        let extracted_image_store = config.extracted_image_store();

        if let Some(image) = extracted_image_store.get_extracted_image(&self.config.image_name()) {
            Ok(image)
        } else {
            let image_store = config.image_store();
            let image = image_store
                .get_image(&self.config.image_name)
                .ok_or(ImageError::InvalidImage)
                .map_err(ContainerError::InitializationError)?;

            Ok(image
                .extract_to(extracted_image_store.path().join(image.name()))
                .map_err(ContainerError::InitializationError)?)
        }
    }

    /// Execute a command in the container
    pub fn run_command(&self, config: &Config, command: &str) -> Result<(), ContainerError> {
        let image = self.extract_image(config)?;
        let c_args = [
            CString::new("/bin/sh").unwrap(),
            CString::new("-c").unwrap(),
            CString::new(command).map_err(|_| ContainerError::InvalidCommand)?,
        ];

        const STACK_SIZE: usize = 1024 * 1024;
        let ref mut stack: [u8; STACK_SIZE] = [0; STACK_SIZE];

        let run_container = move || {
            let result: Result<(), Error> = try {
                // Setup control groups
                self.setup_cpu_cgroup()
                    .with_context(|_| format_err!("cannot setup a CPU cgroup"))?;
                self.setup_memory_cgroup()
                    .with_context(|_| format_err!("cannot setup a memory cgroup"))?;

                sethostname(self.config.name())?;

                mount::<Path, Path, Path, Path>(
                    None,
                    &Path::new("/"),
                    None,
                    MsFlags::MS_PRIVATE | MsFlags::MS_REC,
                    None,
                )
                .with_context(|_| format_err!("cannot mount"))?;

                // Setup OverlayFS with the image directory under an empty read-write directory
                self.setup_overlay(&image).with_context(|_| {
                    format_err!("cannot setup the container's root filesystem")
                })?;

                // Mount basic filesystems (procfs, sysfs, etc)
                self.mount_kernel_filesystems()
                    .with_context(|_| format_err!("cannot mount kernel-related filesystems"))?;

                // Create basic devices (/dev/{null,zero,urandom}, etc)
                self.create_devices()
                    .with_context(|_| format_err!("cannot create devices"))?;

                // Chroot and change directory to isolate the container
                self.move_to_new_root()
                    .with_context(|_| format_err!("cannot move to new root"))?;

                // Detach the old root and remove it
                let old_root = Path::new("/old_root");
                umount2(old_root, MntFlags::MNT_DETACH)
                    .with_context(|_| format_err!("cannot unmount the old root"))?;
                fs::remove_dir(&old_root)
                    .with_context(|_| format_err!("cannot remove the old root"))?;

                // Execute the contained process
                execv(&c_args[0], &c_args)?;
            };

            if let Err(ref e) = result {
                let fail = e.as_fail();
                eprint!("error: {}", fail);
                for cause in fail.iter_causes() {
                    eprint!(": {}", cause);
                }
                eprintln!();

                std::process::exit(242);
            }
            0
        };

        // Create a new process and make the appropriate namespaces
        let pid = clone(
            Box::new(run_container),
            stack,
            CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWUTS | CloneFlags::CLONE_NEWNS,
            Some(SIGCHLD as i32),
        )
        .map_err(ContainerError::ContainerExecutionError)?;

        let status = waitpid(pid, None).map_err(ContainerError::ContainerExecutionError)?;

        match status {
            WaitStatus::Exited(_, 0) => Ok(()),
            WaitStatus::Exited(_, 242) => Err(ContainerError::ContainerSetupError),
            WaitStatus::Exited(_, result) => Err(ContainerError::CommandExitedWithError(result)),
            _ => Err(ContainerError::ContainerExitedAbnormally),
        }
    }

    /// Append files recursively, ignoring special files
    fn append_files<T: Write>(
        &self,
        tar: &mut tar::Builder<T>,
        src_path: &Path,
    ) -> Result<(), std::io::Error> {
        let mut stack = vec![(src_path.to_path_buf(), true)];

        while let Some((src, is_dir)) = stack.pop() {
            let dest = src.strip_prefix(&src_path).unwrap().to_path_buf();
            if is_dir {
                for entry in fs::read_dir(&src)? {
                    let entry = entry?;
                    let file_type = entry.file_type()?;
                    if file_type.is_file() || file_type.is_dir() || file_type.is_symlink() {
                        stack.push((entry.path(), file_type.is_dir()));
                    }
                }
                if dest != Path::new("") {
                    tar.append_dir(&dest, &src)?;
                }
            } else {
                tar.append_path_with_name(&src, &dest)?;
            }
        }
        Ok(())
    }

    /// Export the container as an image
    pub fn export_as_image(&self, config: &Config, name: String) -> Result<Image, ContainerError> {
        let image = self.extract_image(config)?;

        self.setup_overlay(&image)?;

        let rootfs_path = self.path.join("rootfs");
        let temp_archive_path = Path::new("/tmp/image.tar.gz");

        let archive_result: Result<_, std::io::Error> = try {
            // Build an archive with the container's filesystem tree
            let tar_gz = fs::File::create(temp_archive_path)?;
            let enc = GzEncoder::new(tar_gz, Compression::default());
            let mut tar = tar::Builder::new(enc);
            tar.follow_symlinks(false);
            self.append_files(&mut tar, &rootfs_path)?
        };
        archive_result.map_err(ContainerError::ArchiveError)?;

        // Unmount the container's filesystem
        umount(&rootfs_path).map_err(|_| ContainerError::ContainerSetupError)?;

        // Create an image from the archive
        let image_store = config.image_store();
        let image = image_store
            .import_image(name, temp_archive_path)
            .map_err(ContainerError::ExportError)?;
        fs::remove_file(temp_archive_path).map_err(ContainerError::ArchiveError)?;

        Ok(image)
    }
}

/// Structure representing a handle over a directory storing jocker containers
pub struct ContainerStore<'a> {
    containers_dir: &'a Path,
}

impl<'a> ContainerStore<'a> {
    /// Create an [`ExtractedImageStore`] from a path
    pub fn from_directory(containers_dir: &'a Path) -> Self {
        Self { containers_dir }
    }

    /// Retrieve the path to the root directory for this store
    pub fn path(&self) -> &Path {
        &self.containers_dir
    }

    /// Obtain an iterator over the containers available in this store
    pub fn containers(
        &self,
    ) -> Result<impl Iterator<Item = Result<Container, ContainerError>>, std::io::Error> {
        let entries = std::fs::read_dir(self.containers_dir)?;

        Ok(entries.map(|e| {
            e.map_err(ContainerError::InvalidContainerDirectory)
                .and_then(|entry| Container::from_directory(entry.path()))
        }))
    }

    /// Create a container with a name and a base image
    pub fn create_container(
        &self,
        name: String,
        image_name: String,
    ) -> Result<Container, ContainerError> {
        let path = self.containers_dir.join(&name);

        Container::create(name, path, image_name)
    }

    /// Get a handle over a specific container in this store
    pub fn get_container(&self, name: &str) -> Option<Container> {
        let path = self.containers_dir.join(name);

        if path.exists() {
            if let Ok(container) = Container::from_directory(path) {
                Some(container)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Remove a given container from this store
    pub fn remove_container(&self, container: Container) -> Result<(), std::io::Error> {
        // Remove the directory for this container
        fs::remove_dir_all(container.path())
    }
}
