use std::fs;
use std::path::{Path, PathBuf};

use failure::Fail;
use flate2::read::GzDecoder;
use tar::Archive;

#[derive(Fail, Debug)]
pub enum ImageError {
    /// An image could not be used because it is invalid
    #[fail(display = "invalid image")]
    InvalidImage,

    /// An image could not be unpacked
    #[fail(display = "unable to unpack image: {}", _0)]
    UnpackError(std::io::Error),

    /// An intermediate directory could not be created in the cache
    #[fail(display = "unable to create directory: {}", _0)]
    CannotCreateDirectory(std::io::Error),

    /// A tarball could not be imported in the cache
    #[fail(display = "unable to import tarball: {}", _0)]
    CannotImportTarball(std::io::Error),

    /// An image could not be removed
    #[fail(display = "unable to remove image: {}", _0)]
    CannotRemoveImage(std::io::Error),
}

/// Structure representing a handle over a jocker image stored at a given path
#[derive(Debug)]
pub struct Image {
    path: PathBuf,
}

impl Image {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Retrieve the name of the image
    pub fn name(&self) -> &Path {
        Path::new(self.path.file_stem().expect("invalid image path"))
    }

    /// Retrieve the path to the image
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Extract the content of the image to the given directory
    pub fn extract_to<T: AsRef<Path>>(&self, dest_path: T) -> Result<ExtractedImage, ImageError> {
        let dest_path = dest_path.as_ref();
        let file = std::fs::File::open(self.path.join("image.tar.gz"))
            .map_err(|_| ImageError::InvalidImage)?;
        let mut archive = Archive::new(GzDecoder::new(file));

        archive.unpack(dest_path).map_err(ImageError::UnpackError)?;
        Ok(ExtractedImage::new(dest_path.to_path_buf()))
    }
}

/// Structure representing a handle over a directory storing jocker images
#[derive(Debug)]
pub struct ImageStore<'a> {
    images_dir: &'a Path,
}

impl<'a> ImageStore<'a> {
    /// Create an [`ImageStore`] from a path
    pub fn from_directory(images_dir: &'a Path) -> Self {
        Self { images_dir }
    }

    /// Retrieve the path to the root directory for this store
    pub fn path(&self) -> &Path {
        &self.images_dir
    }

    /// Obtain an iterator over the images available in this store
    pub fn images(
        &self,
    ) -> Result<impl Iterator<Item = Result<Image, std::io::Error>>, std::io::Error> {
        let entries = std::fs::read_dir(self.images_dir)?;

        Ok(entries.map(|e| e.map(|entry| Image::new(entry.path()))))
    }

    /// Get a handle over a specific image in this store
    pub fn get_image(&self, image_name: &str) -> Option<Image> {
        let path = self.images_dir.join(image_name);

        if path.exists() {
            Some(Image::new(path))
        } else {
            None
        }
    }

    /// Import an image from a tarball
    pub fn import_image(&self, name: String, path: &Path) -> Result<Image, ImageError> {
        let image_path = self.images_dir.join(&name);
        fs::create_dir_all(&image_path).map_err(ImageError::CannotCreateDirectory)?;
        fs::copy(path, image_path.join("image.tar.gz")).map_err(ImageError::CannotImportTarball)?;

        Ok(Image::new(image_path))
    }

    /// Duplicate an image
    pub fn copy_image(&self, name: String, image: &Image) -> Result<Image, ImageError> {
        let image_archive_path = image.path().join("image.tar.gz");

        self.import_image(name, &image_archive_path)
    }

    /// Remove an image from the store
    pub fn remove_image(&self, image: Image) -> Result<(), ImageError> {
        fs::remove_dir_all(image.path()).map_err(ImageError::CannotRemoveImage)
    }
}

/// Structure representing a handle over a jocker image extracted at a given path
#[derive(Debug)]
pub struct ExtractedImage {
    path: PathBuf,
}

impl ExtractedImage {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Retrieve the name of the image
    pub fn name(&self) -> &Path {
        Path::new(self.path.file_stem().expect("invalid image path"))
    }

    /// Retrieve the path to the image
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Structure representing a handle over a directory storing extracted jocker images
pub struct ExtractedImageStore<'a> {
    images_dir: &'a Path,
}

impl<'a> ExtractedImageStore<'a> {
    /// Create an [`ExtractedImageStore`] from a path
    pub fn from_directory(images_dir: &'a Path) -> Self {
        Self { images_dir }
    }

    /// Retrieve the path to the root directory for this store
    pub fn path(&self) -> &Path {
        &self.images_dir
    }

    /// Get a handle over a specific image in this store
    pub fn get_extracted_image(&self, image_name: &str) -> Option<ExtractedImage> {
        let path = self.images_dir.join(image_name);

        if path.exists() {
            Some(ExtractedImage::new(path))
        } else {
            None
        }
    }
}
