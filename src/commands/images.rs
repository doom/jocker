use std::io::{BufRead, BufReader};
use std::path::Path;

use clap::ArgMatches;
use failure::{format_err, Error, Fail, ResultExt};

use crate::jocker::container::{Container, ContainerError};
use crate::jocker::image::ImageError;
use crate::jocker::Config;

/// Enumeration for the type of commands allowed in Jockerfiles
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
enum JockerfileCommand {
    Run(String),
}

impl std::fmt::Display for JockerfileCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match &self {
            JockerfileCommand::Run(args) => f.write_fmt(format_args!("RUN {}", args)),
        }
    }
}

/// Error type describing errors related to image building
#[derive(Fail, Debug)]
enum ImageBuildError {
    /// The build script is empty
    #[fail(display = "empty build script")]
    EmptyBuildScript,

    /// The build script did not have a FROM directive
    #[fail(display = "missing FROM directive")]
    MissingFromDirective,

    /// The build script did not have a FROM directive
    #[fail(display = "invalid FROM directive")]
    InvalidFromDirective,

    /// The build script contained an invalid command
    #[fail(display = "invalid command {}", _0)]
    InvalidCommand(String),

    /// The build script contained a command with invalid arguments
    #[fail(display = "invalid arguments, expected {}, got {}", _0, _1)]
    InvalidArguments(u32, u32),

    /// An intermediate container produced an error
    #[fail(display = "error in intermediate container: {}", _0)]
    IntermediateContainerError(ContainerError),

    /// The resulting image could not be created
    #[fail(display = "unable to create the resulting image: {}", _0)]
    CannotCreateResultingImage(ImageError),
}

/// Structure representing an image builder, which allows building jocker images
struct ImageBuilder<T: BufRead> {
    reader: T,
}

impl<T: BufRead> ImageBuilder<T> {
    /// Create an [`ImageBuilder`] from a reader
    pub fn from_reader(reader: T) -> Self {
        Self { reader }
    }

    fn parse_from_directive<'a>(
        lines_iter: &mut impl Iterator<Item = &'a String>,
    ) -> Result<String, ImageBuildError> {
        let mut from_line = lines_iter
            .next()
            .map(|line| line.split_ascii_whitespace())
            .ok_or(ImageBuildError::EmptyBuildScript)?;

        match from_line.next() {
            Some("FROM") => match from_line.next() {
                Some(s) => Ok(s.to_string()),
                _ => Err(ImageBuildError::InvalidFromDirective),
            },
            _ => Err(ImageBuildError::MissingFromDirective),
        }
    }

    fn parse_command(line: &str) -> Result<JockerfileCommand, ImageBuildError> {
        let mut pieces = line.splitn(2, ' ');

        match pieces.next() {
            Some("RUN") => match pieces.next() {
                Some(args) if !args.is_empty() => Ok(JockerfileCommand::Run(args.to_string())),
                _ => Err(ImageBuildError::InvalidArguments(1, 0)),
            },
            Some(cmd) => Err(ImageBuildError::InvalidCommand(cmd.to_string())),
            _ => unreachable!(),
        }
    }

    fn execute_command(
        config: &Config,
        container: &Container,
        command: &JockerfileCommand,
    ) -> Result<(), ImageBuildError> {
        println!("Running \"{}\"...", command);

        match command {
            JockerfileCommand::Run(args) => container
                .run_command(config, &args)
                .map_err(ImageBuildError::IntermediateContainerError),
        }
    }

    /// Build the image
    pub fn build(self, config: &Config, name: Option<String>) -> Result<(), ImageBuildError> {
        let container_store = config.container_store();

        let lines = self.reader.lines().collect::<Result<Vec<_>, _>>().unwrap();
        let mut lines_iter = lines.iter().filter(|s| !s.is_empty());

        let mut base_image = Self::parse_from_directive(&mut lines_iter)?;

        for line in lines_iter {
            let container = container_store
                .create_container(uuid::Uuid::new_v4().to_string(), base_image)
                .map_err(ImageBuildError::IntermediateContainerError)?;

            let command = Self::parse_command(line)?;
            Self::execute_command(config, &container, &command)?;

            let image_name = uuid::Uuid::new_v4().to_string();
            println!("Saving temporary container to image {}...", &image_name);
            container
                .export_as_image(config, image_name.clone())
                .expect("cannot export");
            base_image = image_name;
        }

        if let Some(name) = name {
            let image_store = config.image_store();
            let image = image_store
                .get_image(&base_image)
                .expect("cannot find the built image");
            image_store
                .copy_image(name, &image)
                .map_err(ImageBuildError::CannotCreateResultingImage)?;
        }

        Ok(())
    }
}

pub fn build(config: &Config, matches: &ArgMatches) -> Result<(), Error> {
    let path = Path::new(matches.value_of("PATH").unwrap());
    let file_path = path.join("Jockerfile");

    let file = std::fs::File::open(&file_path).with_context(|_| {
        format_err!("cannot open build script at path {}", file_path.display())
    })?;
    let file = BufReader::new(file);

    let builder = ImageBuilder::from_reader(file);
    builder
        .build(config, matches.value_of("name").map(String::from))
        .with_context(|_| format_err!("cannot build image"))?;

    Ok(())
}

pub fn import(config: &Config, matches: &ArgMatches) -> Result<(), Error> {
    let name = matches.value_of("NAME").unwrap();
    let path = Path::new(matches.value_of("PATH").unwrap());
    let image_store = config.image_store();

    image_store.import_image(name.to_string(), path)?;

    Ok(())
}

pub fn list(config: &Config, matches: &ArgMatches) -> Result<(), Error> {
    let image_store = config.image_store();

    if matches.is_present("quiet") {
        for image in image_store.images()? {
            println!("{}", image?.name().display());
        }
    } else {
        for image in image_store.images()? {
            let image = image?;
            println!("{}: {}", image.name().display(), image.path().display());
        }
    }

    Ok(())
}

pub fn remove(config: &Config, matches: &ArgMatches) -> Result<(), Error> {
    let image_store = config.image_store();

    for image_name in matches.values_of("IMAGE").unwrap() {
        if let Some(image) = image_store.get_image(image_name) {
            image_store.remove_image(image)?;
            println!("{}: removed", image_name);
        } else {
            println!("unable to remove {}: no such image", image_name);
        }
    }

    Ok(())
}
