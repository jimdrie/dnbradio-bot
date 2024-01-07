# DnBRadio Discord and IRC bot
This bot is used to relay messages between the DnBRadio Discord and IRC channels. It is written in Rust and uses the
[serenity](https://github.com/serenity-rs/serenity) Discord crate and the [irc](https://github.com/aatxe/irc) IRC
crate. The Shazam code is based on [SongRec](https://github.com/marin-m/SongRec), modified to work with async, to work
directly from a stream URL and some issues resolved.


## Configuration
The configuration is done through environment variables, which can be set in a `.env` file, see .env.example.


## Usage
To build the application, you need to have the [Rust toolchain](https://www.rust-lang.org/tools/install) installed.
You can then run the application using `cargo run`. To build a release version, use `cargo build --release`.

A Dockerfile and compose.yaml file are included for use with Docker. These are also used to run it in production.


## License
This software is released under the GNU GPL v3 license. See the LICENSE file for more information.
