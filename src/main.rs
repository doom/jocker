#![feature(try_blocks)]
#![feature(type_ascription)]
#![allow(dead_code)]

use clap::{crate_name, App, AppSettings, Arg, SubCommand};

mod commands;
mod jocker;

fn main() {
    let app = App::new(crate_name!())
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .setting(AppSettings::VersionlessSubcommands)
        .setting(AppSettings::ColoredHelp)
        .subcommand(
            SubCommand::with_name("container")
                .about("Manage existing containers")
                .setting(AppSettings::SubcommandRequiredElseHelp)
                .subcommand(
                    SubCommand::with_name("ls")
                        .about("list existing containers")
                        .arg(
                            Arg::with_name("quiet")
                                .help("only list container names")
                                .short("q")
                                .long("quiet"),
                        ),
                )
                .subcommand(
                    SubCommand::with_name("rm")
                        .about("remove existing containers")
                        .arg(
                            Arg::with_name("CONTAINER")
                                .help("the containers to remove")
                                .required(true)
                                .multiple(true),
                        ),
                )
                .subcommand(
                    SubCommand::with_name("start")
                        .about("run a command in an existing stopped container")
                        .arg(
                            Arg::with_name("CONTAINER")
                                .help("the container to run")
                                .required(true),
                        )
                        .arg(
                            Arg::with_name("COMMAND")
                                .help("the command to run in a container")
                                .required(true),
                        )
                        .arg(
                            Arg::with_name("ARG")
                                .help("the arguments to pass to the command")
                                .required(false)
                                .multiple(true),
                        ),
                ),
        )
        .subcommand(
            SubCommand::with_name("image")
                .about("Manage images")
                .setting(AppSettings::SubcommandRequiredElseHelp)
                .subcommand(
                    SubCommand::with_name("build")
                        .about("build a new image")
                        .arg(
                            Arg::with_name("name")
                                .help("the name to give to the resulting image")
                                .short("t")
                                .takes_value(true)
                                .required(false),
                        )
                        .arg(
                            Arg::with_name("PATH")
                                .help("the path to the directory containing the build files")
                                .required(true),
                        ),
                )
                .subcommand(
                    SubCommand::with_name("import")
                        .about("import an image from a tarball")
                        .arg(
                            Arg::with_name("NAME")
                                .help("the name to give to the image")
                                .required(true),
                        )
                        .arg(
                            Arg::with_name("PATH")
                                .help("the path to the tarball to import")
                                .required(true),
                        ),
                )
                .subcommand(
                    SubCommand::with_name("ls")
                        .about("list existing images")
                        .arg(
                            Arg::with_name("quiet")
                                .help("only list image names")
                                .short("q")
                                .long("quiet"),
                        ),
                )
                .subcommand(
                    SubCommand::with_name("rm")
                        .about("remove existing images")
                        .arg(
                            Arg::with_name("IMAGE")
                                .help("the images to remove")
                                .required(true)
                                .multiple(true),
                        ),
                ),
        )
        .subcommand(
            SubCommand::with_name("run")
                .about("Create and run containers")
                .arg(
                    Arg::with_name("name")
                        .help("the name to use for this container")
                        .long("name")
                        .takes_value(true)
                        .required(false),
                )
                .arg(
                    Arg::with_name("IMAGE")
                        .help("the image to use as base for the container")
                        .required(true),
                )
                .arg(
                    Arg::with_name("COMMAND")
                        .help("the command to run in a container")
                        .required(true),
                )
                .arg(
                    Arg::with_name("ARG")
                        .help("the arguments to pass to the command")
                        .required(false)
                        .multiple(true),
                ),
        );

    let matches = app.get_matches();

    let config = jocker::Config::new(
        &dirs::home_dir()
            .expect("unable to get home directory")
            .join(".jocker"),
    );

    let result = match matches.subcommand() {
        ("container", Some(matches)) => match matches.subcommand() {
            ("ls", Some(matches)) => commands::containers::list(&config, matches),
            ("rm", Some(matches)) => commands::containers::remove(&config, matches),
            ("start", Some(matches)) => commands::containers::start(&config, matches),
            _ => unimplemented!(),
        },
        ("image", Some(matches)) => match matches.subcommand() {
            ("build", Some(matches)) => commands::images::build(&config, matches),
            ("import", Some(matches)) => commands::images::import(&config, matches),
            ("ls", Some(matches)) => commands::images::list(&config, matches),
            ("rm", Some(matches)) => commands::images::remove(&config, matches),
            _ => unimplemented!(),
        },
        ("run", Some(matches)) => commands::run(&config, matches),
        _ => unimplemented!(),
    };

    if let Err(e) = result {
        let fail = e.as_fail();
        eprint!("error: {}", fail);
        for cause in fail.iter_causes() {
            eprint!(": {}", cause);
        }
        eprintln!();

        std::process::exit(1);
    }
}
