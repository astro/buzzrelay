# Objective
Install the FediBuzz relay using a virtual machine running Ubuntu Server 22.04.4 LTS. If you are familiar with NixOS/Flakes, then [installing the NixOS module](https://github.com/astro/buzzrelay?tab=readme-ov-file#build) for this is by far the easier route! 

But for those of us unfamiliar with NixOS, here's another option. 

# Server Configuration
My system consisted of the following:

* Router Configured to forward 80, 443 through a firewall to NGINX Proxy Manager
* NGINX Proxy Manager then directs traffic based on the incoming domain name to the appropriate server.
* A ProxMox server with an Ubuntu VM hosting the FediBuzz relay application.

## Hardware
This could be run very cheaply on your preferred hosting site, or on your own home lab.

* Ubuntu 22.04.4 LTS 
* 1 Core
* 1 GB RAM

The official fedi.buzz relay traffics around 330 public instance streams for around 3,500 unique followers (some may have multiple relay requests) as of April 2024 with a similar configuration.

If you're connecting to the fedi.buzz relay and perhaps one or two others on your own relay, this should be more than enough.

One caveat here. FediBuzz has an option for individual users to utilize relays as well by "following" a relay actor, like `@tag-dogsofmastodon@relay.com`. 

If you promote this alternative route and many individuals start connecting to your relay, it will cause more outgoing traffic and queue processing, therefore increasing your hardware requirements.

# Domain Name
As with most fediverse projects, you're going to need a domain. In this particular instance, I used a subdomain of https://relay.{yourdomain}.

# NGINX Proxy Manager Config 
You'll need an SSL certificate for at least the sub-domain. NGINX Proxy Manager has this built in, but you may need some additional assistance depending on your configuration.

I needed a wildcard SSL for my domain, and used the PorkBun API to make that work. Here's a [blog post about Cloudflare](https://blog.jverkamp.com/2023/03/27/wildcard-lets-encrypt-certificates-with-nginx-proxy-manager-and-cloudflare/), but the setup is very similar for PorkBun.

Websockets may not be required, but I enabled it in NGINX Proxy Manager during configuration.
 
# Firewall
The following ports need to be open on the server running FediRelay. 

```
## Default is 3000 in the FediBuzz docs, change as needed
sudo ufw allow 3000
## Allow SSH traffic so you can connect - consider limiting to specific IPs
sudo ufw allow 22

## When ready...
sudo ufw enable
```

# Initial Server Installs
These packages are required for rust / cargo to work.
```
sudo apt-get update
sudo apt install pkg-config
sudo apt-get install libssl-dev
sudo apt-get install libsystemd-dev
sudo apt install git cargo
sudo apt install curl
```

curl was already installed for me.

### Rust and related tooling install
Rust is already installed on Ubuntu, but not compatible with rustup. Remove it.

```
sudo apt remove rustc cargo
sudo apt autoremove
```

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Just installed the default (1) option. This could take a LONG while. 

Then set your system's PATH environment variable to make Rust's tools globally accessible:

```
source $HOME/.cargo/env
```

# Pull GitHub Repo 
```
git clone https://github.com/astro/buzzrelay.git
```

# Postgres SQL
Install a PostGresSQL database, I followed this guide.

[https://www.digitalocean.com/community/tutorials/how-to-install-and-use-postgresql-on-ubuntu-22-04](https://www.digitalocean.com/community/tutorials/how-to-install-and-use-postgresql-on-ubuntu-22-04)

Then I create the relay user for the database.

```
createuser --interactive
```

Role (user) to add: relay as superuser
Password wasn't set, so from a postgres prompt:

`ALTER USER relay WITH PASSWORD 'your-secure-password';`

Then create database: 

`CREATE DATABASE buzzrelay;`

Then grant the relay user with full rights to the database:

```
ALTER DATABASE buzzrelay OWNER TO relay;
```

```
GRANT ALL PRIVILEGES ON DATABASE buzzrelay TO relay;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO relay;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO relay;
GRANT ALL PRIVILEGES ON ALL FUNCTIONS IN SCHEMA public TO relay;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL PRIVILEGES ON TABLES TO relay; 
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL PRIVILEGES ON SEQUENCES TO relay; 
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL PRIVILEGES ON FUNCTIONS TO relay;
GRANT USAGE, CREATE ON SCHEMA public TO relay;
```

## Querying the database
A cheat sheet for getting to the database.

```
sudo -i -u postgres
psql
\c buzzrelay
```

# Redis
It's not necessary to install this, it is not used by the core relay. Just comment out the associated lines in the YAML file.

This was used if you are going to host the page shown at [https://fedi.buzz](https://fedi.buzz) which doesn't come with this relay configuration.

# Update config.yaml
Several items need to be updated, see below

## Streams
* Leave the fedibuzz stream as is.
* Comment out the first example.
* Change the last example to your instance's url and token.
* Add as many others as desired.
 
## Additional filters for streams
If you have a token for an instance that you are using to connect to a mastodon public stream, you're not limited to just the federated stream of all posts. If you want to get more granular, these streaming timelines work, too.

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
### Hostname
Set it to your domain. I used the sub-domain format of "relay.{yourdomain}"
### Private Key File
Generate a new RSA key pair for signing ActivityPub messages. Note using this command also sets the appropriate permissions values.

```bash
openssl genrsa -out private-key.pem 4096 
openssl rsa -in private-key.pem -pubout -out public-key.pem
```

## PostGres Password
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

With the fedi relay public stream enabled, I see the following error stream quite often, showing that the uri is missing, which it is.

```
2024-03-23T03:39:34.773184Z TRACE buzzrelay::relay: data: {"created_at":"2024-03-23T03:39:33.020Z","url":"https://some.instance/notes/9r73vj18yk","content":"<p><a href=\"https://some.instance/@some.user\" class=\"u-url mention\">@some.user</a><span> Some Content​</p>","account":{"username":"some.user","display_name":"some.display.name","url":"https://some.instance/@some.user","bot":true},"tags":[],"sensitive":false,"mentions":[],"language":"ja","media_attachments":[],"reblog":null}
2024-03-23T03:39:48.745870Z ERROR buzzrelay::relay: parse error: missing field `uri` at line 1 column 746
```

However, even with that error, content is coming in for at least the hashtag and instance relay types.

# Use it
And with that, I had a running relay! Check the homepage of your relay for instructions on how to get started. 

Congratulations! 🎉

# Next steps
Check out the /metrics endpoint at {yourRelayDomain}/metrics for information about the current status of your relay.

Additionally, once you stabilize the settings, you may want to set this up to run automatically after a system reboot.

```ssh
sudo nano /etc/systemd/system/buzzrelay.service
```

Edit the new service file, change the working directory to your own location.

```nano
[Unit]
Description=Buzzrelay Rust Application
After=network.target

[Service]
Type=simple
User=box464
WorkingDirectory=/home/box464/buzzrelay
ExecStart=/home/box464/.cargo/bin/cargo run --release /home/box464/buzzrelay/config.yaml
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Finally, enable the service, start it, and check the status.

```ssh
sudo systemctl daemon-reload
sudo systemctl enable buzzrelay.service
sudo systemctl start buzzrelay.service
sudo systemctl status buzzrelay.service

```
