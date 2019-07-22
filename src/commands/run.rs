use clap::ArgMatches;
use failure::Error;
use uuid::Uuid;

use crate::jocker::Config;

pub fn run(config: &Config, matches: &ArgMatches) -> Result<(), Error> {
    let container_id = if let Some(name) = matches.value_of("name") {
        name.chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .collect()
    } else {
        Uuid::new_v4().to_string()
    };
    let image_name = matches.value_of("IMAGE").unwrap();

    println!(
        "Creating container with ID {} from image {}",
        container_id, image_name
    );
    let container_store = config.container_store();
    let container =
        container_store.create_container(container_id.clone(), image_name.to_string())?;

    println!("Running container with ID {}", container_id);
    let mut cmd_args = Vec::new();
    cmd_args.push(matches.value_of("COMMAND").unwrap());
    if let Some(args) = matches.values_of("ARG") {
        cmd_args.extend(args);
    }
    let cmd = cmd_args.join(" ");

    container.run_command(config, &cmd)?;

    Ok(())
}
