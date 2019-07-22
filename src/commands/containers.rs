use clap::ArgMatches;
use failure::Error;

use crate::jocker::Config;

pub fn list(config: &Config, _matches: &ArgMatches) -> Result<(), Error> {
    let container_store = config.container_store();

    for container in container_store.containers()? {
        println!("{}", container?.name());
    }

    Ok(())
}

pub fn remove(config: &Config, matches: &ArgMatches) -> Result<(), Error> {
    let container_store = config.container_store();

    for container_name in matches.values_of("CONTAINER").unwrap() {
        let container = container_store.get_container(container_name);

        if let Some(container) = container {
            container_store.remove_container(container)?;
            println!("{}: removed", container_name);
        } else {
            println!("unable to remove {}: no such container", container_name);
        }
    }

    Ok(())
}

pub fn start(config: &Config, matches: &ArgMatches) -> Result<(), Error> {
    let container_id = matches.value_of("CONTAINER").unwrap();

    println!("Loading container with ID {}", container_id);
    let container_store = config.container_store();
    let container = container_store.get_container(&container_id).unwrap();

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
