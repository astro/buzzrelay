# Objective
This guide will provide you with a working relay to test and configure to your liking. 

If you are familiar with NixOS/Flakes, then [installing the NixOS module](https://github.com/astro/buzzrelay?tab=readme-ov-file#build) is by far the easier route! 

If you're not ready to take the NixOS plunge, here's another option to install the FediBuzz relay on server with a recent release of Ubuntu. 

## Hardware
The official buzzrelay is attached to hundreds of instances and has thousands of followers with a configuration similar to the requirements listed below.

* 1 Core
* 1 GB RAM

If you're connecting to the fedi.buzz relay and perhaps one or two others on your own relay, this should be more than enough.

One caveat here. FediBuzz has an option for individual users to utilize relays as well by "following" a relay actor, like `@tag-dogsofmastodon@relay.com`. 

If you promote this alternative route and many individuals start connecting to your relay, it will cause more outgoing traffic and queue processing, therefore increasing your hardware requirements.

# Domain Name
You'll need a domain or subdomain to run this application. For example, a subdomain of `https://relay.{yourdomain}`.

# SSL Certificate
You'll need an SSL certificate for your domain. 

# Initial Server Installs
These packages are required for rust / cargo to work.
```
sudo apt-get update
sudo apt-get install pkg-config libssl-dev libsystemd-dev git cargo curl
```

## Rust and related tooling install
Ensure Rust is installed on your server. Ubuntu has a rustc installation included by default, but it is likely not the latest version. In addition, you may prefer to use rustup to manage your  install. Check out [the official installation guide](https://www.rust-lang.org/tools/install).

## Pull GitHub Repo 
```
git clone https://github.com/astro/buzzrelay.git
```

## PostgreSQL
A PostgreSQL database is needed for this application. This [installation guide](https://www.digitalocean.com/community/tutorials/how-to-install-and-use-postgresql-on-ubuntu-22-04) will assist with the initial install.

Additionally, you'll have to allow for password authentication. [This guide](https://medium.com/@syafiqza/configuring-postgresql-authentication-in-linux-from-peer-to-password-1bde0445c4da) walks you through the process.

Create the relay user for the database. Specifically this command creates a user named relay and then prompts for a password.

```
sudo -u postgres createuser -P relay
```

Then create the database and grant all prviliges to the relay user. 

```
sudo -u postgres psql 
createdb -O relay buzzrelay

\c buzzrelay

GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO relay;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO relay;
GRANT ALL PRIVILEGES ON ALL FUNCTIONS IN SCHEMA public TO relay;

ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL PRIVILEGES ON TABLES TO relay;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL PRIVILEGES ON SEQUENCES TO relay;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL PRIVILEGES ON FUNCTIONS TO relay;
```

## Querying the database
A cheat sheet for getting to the database.

```
psql -U relay -h localhost -d buzzrelay
\c buzzrelay
```

# Redis
It's not necessary to install this, it is not used by the core relay. Just comment out the associated lines in the YAML file.

This was used if you are going to host the page shown at [https://fedi.buzz](https://fedi.buzz) which doesn't come with this relay configuration.

# Update config.yaml

## Streams
* Leave the fedibuzz stream as is.
* Comment out the first example.
* Change the last example to your instance's url and token.
* Add as many others as desired.
 
## Additional filters for streams
If you have a token for an instance that you are using to connect to a mastodon public stream, you're not limited to just the federated stream of all posts. If you want to get more granular, these [streaming timelines](https://docs.joinmastodon.org/methods/streaming/) work, too.

<details><summary><b>View additional filter details</b></summary>
All of the items listed below have a /local/ version as well if you want to get REALLY granular and only pick up posts from the local instance.

> This does not work for the default fedibuzz relay stream, only for mastodon servers for which you have an access token.

**Public remote posts only - federated posts excluding local ones**

You can also pass a "only_media" parameter in the querystring and get back only posts with some type of attachment (audio, image, or video) Cool!

```http
GET /api/v1/streaming/public/remote?only_media={true|false}&access_token={yourAccessToken} HTTP/1.1
```

**Public posts with a specific hashtag**

>This one does not has the only_media parameter unfortunately.

```http
GET /api/v1/streaming/hashtag?tag={yourTag}&access_token={yourAccessToken} HTTP/1.1
```

**Watch a list for posts**
For the user with the associated token, you can create a list of accounts and pass the list_id to this query. It will return only posts from those accounts.

```http
GET /api/v1/streaming/list?list={yourListId}&access_token={yourAccessToken} HTTP/1.1
```
</details>

### Hostname
Set it to your domain. I used the sub-domain format of "relay.{yourdomain}"

### Listen Port
Update if necessary for your proxy configuration.

### Private Key File
Generate a new RSA key pair for signing ActivityPub messages. Note using this command also sets the appropriate permissions values.

```bash
openssl genrsa -out private-key.pem 4096 
openssl rsa -in private-key.pem -pubout -out public-key.pem
```

## PostgreSQL Password
I used the default user and dbname listed in the config file. Update the password as needed.

# Build it
From the root of the buzzrelay project, with the config.yaml finalized, run the following.

```
cargo build --release
```

This creates a compiled version in the target/release folder.

From the root of the project, you can run this command to start up the app:

```
cargo run --release config.yaml
```

If you see redis errors, be sure to comment out those lines in the config.yaml - it is NOT needed.

With the fedi relay public stream enabled, I did see the following error stream quite often, showing that the uri is missing, which it is.

```
2024-03-23T03:39:34.773184Z TRACE buzzrelay::relay: data: {"created_at":"2024-03-23T03:39:33.020Z","url":"https://some.instance/notes/9r73vj18yk","content":"<p><a href=\"https://some.instance/@some.user\" class=\"u-url mention\">@some.user</a><span> Some Content​</p>","account":{"username":"some.user","display_name:":"some.display.name","url":"https://some.instance/@some.user","bot":true},"tags":[],"sensitive":false,"mentions":[],"language":"ja","media_attachments":[],"reblog":null}
2024-03-23T03:39:48.745870Z ERROR buzzrelay::relay: parse error: missing field `uri` at line 1 column 746
```

However, even with that error, plenty of content is getting pushed to my instance.

# Try it out
Check the homepage of your new relay for instructions on how to add your desired entries to a fediverse server and start pulling in posts. 

You should see entries being added to your federated timeline.

You've got a basic working relay to test with. Congratulations! 🎉