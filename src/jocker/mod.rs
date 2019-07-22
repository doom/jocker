use std::path::{Path, PathBuf};

pub mod container;
pub mod image;

pub struct Config {
    container_store_path: PathBuf,
    extracted_image_store_path: PathBuf,
    image_store_path: PathBuf,
}

impl Config {
    /// Create a new configuration from a base directory
    pub fn new(base_dir: &Path) -> Self {
        let container_store_path = base_dir.join("containers");
        let extracted_image_store_path = base_dir.join("extracted_images");
        let image_store_path = base_dir.join("images");

        Self {
            container_store_path,
            extracted_image_store_path,
            image_store_path,
        }
    }

    /// Obtain a handle over the image store
    pub fn image_store(&self) -> image::ImageStore {
        image::ImageStore::from_directory(&self.image_store_path)
    }

    /// Obtain a handle over the extracted image store
    pub fn extracted_image_store(&self) -> image::ExtractedImageStore {
        image::ExtractedImageStore::from_directory(&self.extracted_image_store_path)
    }

    /// Obtain a handle over the container store
    pub fn container_store(&self) -> container::ContainerStore {
        container::ContainerStore::from_directory(&self.container_store_path)
    }
}
