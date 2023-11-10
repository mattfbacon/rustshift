# Rustshift

A blue light filter thingy for Wayland (zwlr-gamma-control-v1 protocol).

## Usage

All parameters are hard-coded. Just run with `cargo run`. Make sure you don't have another gamma manager running.

Also, it currently does not work with multiple monitors or any kind of monitor hot-plugging because I haven't figured out how to get notified when monitors are connected.

## License

AGPL-3.0-or-later
