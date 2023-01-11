# buzzrelay

A follow-only ActivityPub relay that connects to Mastodon's [Streaming
API](https://docs.joinmastodon.org/methods/streaming/#public).

You don't need to run this yourself, just use the instance at
[relay.fedi.buzz](https://relay.fedi.buzz/).

## Setup

### Build

NixOS/Flakes users are in luck: not only does this build, it also
comes with a NixOS module!

Anyone else installs a Rust toolchain to build with:

```bash
cargo build --release
```

### Generate signing keypair

ActivityPub messages are signed using RSA keys. Generate a keypair
first:

```bash
openssl genrsa -out private-key.pem 4096
openssl rsa -in private-key.pem -pubout -out public-key.pem
```

Let your `config.yaml` point there.

### Database

Create a PostgreSQL database and user, set them in your `config.yaml`.

The program will create its schema on start.

## Ethics

*Should everyone connect to the streaming API of the big popular
Mastodon instances?*

Once these connections become a problem, they may become disallowed,
resulting in problems for everyone. That's why **fedi.buzz** serves
the firehose feed through the streaming API, too.

You can let this service use **fedi.buzz** as listed in the default
`config.yaml`.
