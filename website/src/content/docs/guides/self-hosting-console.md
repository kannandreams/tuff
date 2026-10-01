---
title: Self-Hosting the Console
description: Run Tuff Console on a Linux server with systemd or in a container, put a reverse proxy in front of it, and back it up, upgrade it, and manage its keys.
---

This guide runs [Tuff Console](/cli/console/) for a team: one `tuff` process, one data folder, and a reverse proxy that authenticates the people who view it. The examples use the organisation `acme` and the address `https://tuff.acme.dev`.

The console is part of the `tuff` binary. `tuff console serve` needs `tuff` from a release that includes the console. Check with `tuff console --help`.

## How the pieces fit

| Piece | Setting |
|---|---|
| The process | `tuff console serve`, listening on loopback behind a proxy, or on `0.0.0.0` in a container |
| The data | One SQLite file, `console.sqlite`, in the folder given with `--data` |
| Publishers | GitHub Actions jobs through `--trust github:<owner>`, or any CI system through an API key |
| Viewers | Authenticated by the reverse proxy. The console does not authenticate viewers |
| The public address | `--public-url`, which is what publishers put in `TUFF_CONSOLE_URL` |

The console checks credentials on `POST /api/v1/reports` only. Everything else is read only and open to whoever can reach the port, so the port must be reachable from the proxy and nowhere else.

## Run with systemd

Install `tuff` on the server, create a user for the service, and write the unit. [Installation](/installation/) lists the ways to install `tuff`. The unit below expects `/usr/local/bin/tuff`, where the curl installer puts it.

```sh frame="terminal"
sudo useradd --system --home-dir /var/lib/tuff-console --shell /usr/sbin/nologin tuff
```

```ini title="/etc/systemd/system/tuff-console.service"
[Unit]
Description=Tuff Console
After=network-online.target
Wants=network-online.target

[Service]
User=tuff
Group=tuff
StateDirectory=tuff-console
StateDirectoryMode=0750
ExecStart=/usr/local/bin/tuff console serve \
  --addr 127.0.0.1:7474 \
  --data /var/lib/tuff-console \
  --trust github:acme \
  --public-url https://tuff.acme.dev
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

`StateDirectory=tuff-console` makes systemd create `/var/lib/tuff-console`, owned by `tuff`, and keep it writable under `ProtectSystem=strict`. The console creates `console.sqlite` inside it with mode `0600`.

```sh frame="terminal"
sudo systemctl daemon-reload
sudo systemctl enable --now tuff-console
journalctl -u tuff-console -n 5
curl http://127.0.0.1:7474/healthz
```

The journal shows the startup lines: the address, the data folder, and the OIDC audience. The audience must equal the public URL that publishers use.

The server stops on `SIGTERM`, which is what `systemctl stop` sends, and on `SIGINT`.

The unit binds `127.0.0.1`, so `--public-read` is not needed. Bind a non-loopback address only when the proxy runs on another host. Then add `--public-read`, and make sure a firewall lets only the proxy connect.

A server with no `--trust` and no key accepts reports from anything that can reach it, including the proxy. The unit above has a trust. A console that publishes from other CI systems also needs a key, created as described in [Keys on the host](#keys-on-the-host).

## Run in a container

The release workflow attaches `tuff-x86_64-unknown-linux-gnu.tar.gz` and `checksums.txt` to each GitHub release. The tarball holds the `tuff` binary. This Dockerfile downloads it, checks its checksum, and runs the console as an unprivileged user with `/data` as a volume:

```dockerfile title="Dockerfile"
FROM ubuntu:24.04

ARG TUFF_VERSION
ARG BASE=https://github.com/kannandreams/tuff/releases/download/v${TUFF_VERSION}

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && curl -fsSLO ${BASE}/tuff-x86_64-unknown-linux-gnu.tar.gz \
 && curl -fsSLO ${BASE}/checksums.txt \
 && grep ' tuff-x86_64-unknown-linux-gnu.tar.gz$' checksums.txt | sha256sum -c - \
 && tar -xzf tuff-x86_64-unknown-linux-gnu.tar.gz -C /usr/local/bin tuff \
 && rm tuff-x86_64-unknown-linux-gnu.tar.gz checksums.txt \
 && apt-get purge -y curl \
 && apt-get autoremove -y \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home-dir /data --shell /usr/sbin/nologin tuff \
 && mkdir /data \
 && chown tuff:tuff /data

USER tuff
VOLUME /data
EXPOSE 7474
ENTRYPOINT ["tuff", "console", "serve", "--addr", "0.0.0.0:7474", "--public-read", "--data", "/data"]
```

Set `TUFF_VERSION` to a release that includes the console, without the leading `v`. The release has a Linux x86-64 binary only, so build the image for `linux/amd64`. `ca-certificates` stays in the image because the console fetches GitHub's OIDC signing keys over HTTPS.

The entry point binds `0.0.0.0` and passes `--public-read`. The console also refuses to start on a non-loopback address until a publish credential exists, so create a key in the volume before the first start or pass `--trust`:

```sh frame="terminal"
docker build --platform linux/amd64 --build-arg TUFF_VERSION=<version> -t tuff-console .

# Create a key in the volume. The key is printed once
docker run --rm -v tuff-console-data:/data --entrypoint tuff tuff-console \
  console key create ci --data /data

# Start the console. Arguments after the image name are added to the entry point
docker run -d --name tuff-console --restart unless-stopped \
  -p 127.0.0.1:7474:7474 \
  -v tuff-console-data:/data \
  tuff-console --trust github:acme --public-url https://tuff.acme.dev
```

Publishing `127.0.0.1:7474` keeps the port reachable from the host only, where the reverse proxy runs. `GET /healthz` answers for container health checks. The image has no `curl` after the build, so run the check from outside the container.

Upgrade by building a new image with a newer `TUFF_VERSION` and recreating the container with the same volume.

## Put a reverse proxy in front

The console does not authenticate viewers, and viewer sign-in is planned for a later release. Until then the proxy decides who can open the pages and the read API.

Publishing needs different handling. Publishers send `POST /api/v1/reports` with their own `Authorization: Bearer` header, which the console verifies, so the proxy must leave that request to the console. Requiring a login there would reject every publish, and proxy basic auth would replace the `Authorization` header the console reads. Forward the header unchanged, and raise any request body limit to at least 16 MiB, which is the largest report the console accepts.

### Caddy with basic auth

```caddyfile title="Caddyfile"
tuff.acme.dev {
	@publish {
		method POST
		path /api/v1/reports
	}

	handle @publish {
		reverse_proxy 127.0.0.1:7474
	}

	handle {
		basic_auth {
			# caddy hash-password
			alice $2a$14$replace-with-the-output-of-caddy-hash-password
		}
		reverse_proxy 127.0.0.1:7474
	}
}
```

Caddy obtains and renews the certificate for `tuff.acme.dev` by itself.

### Caddy with an authentication service

To sign people in with the organisation's identity provider, run a forward-auth service such as oauth2-proxy and point Caddy at it:

```caddyfile title="Caddyfile"
tuff.acme.dev {
	@publish {
		method POST
		path /api/v1/reports
	}

	handle @publish {
		reverse_proxy 127.0.0.1:7474
	}

	handle {
		forward_auth 127.0.0.1:4180 {
			uri /oauth2/auth
		}
		reverse_proxy 127.0.0.1:7474
	}
}
```

The sign-in redirect and the identity provider are configured in the authentication service. See the Caddy documentation for `forward_auth`.

### nginx

```nginx title="/etc/nginx/conf.d/tuff.conf"
map "$request_method $uri" $tuff_realm {
    default                 "Tuff Console";
    "POST /api/v1/reports"  off;
}

server {
    listen 443 ssl;
    server_name tuff.acme.dev;

    ssl_certificate     /etc/letsencrypt/live/tuff.acme.dev/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/tuff.acme.dev/privkey.pem;

    client_max_body_size 16m;

    location / {
        auth_basic           $tuff_realm;
        auth_basic_user_file /etc/nginx/tuff.htpasswd;
        proxy_pass           http://127.0.0.1:7474;
    }
}
```

Create the password file with `htpasswd -c /etc/nginx/tuff.htpasswd alice`. The `map` turns basic auth off for `POST /api/v1/reports`, which goes straight to the console. Every other request needs the password.

### Checking the proxy

With a key in `TUFF_CONSOLE_KEY`:

```sh frame="terminal"
# No password: the proxy refuses
curl -i https://tuff.acme.dev/api/v1/projects

# A publish reaches the console, which checks the key itself
TUFF_CONSOLE_URL=https://tuff.acme.dev tuff console publish --dry-run
TUFF_CONSOLE_URL=https://tuff.acme.dev tuff console publish
```

`TUFF_CONSOLE_URL` must equal `--public-url`, apart from a trailing slash, for GitHub Actions publishing to work. The OIDC token names that address as its audience.

## Keys on the host

Keys are managed with `tuff console key`, run on the host against the data folder. The commands work while the server runs, and a revoked key stops working on the next request.

```sh frame="terminal"
sudo -u tuff tuff console key create billing-ci --repository github.com/acme/agents --data /var/lib/tuff-console
sudo -u tuff tuff console key list --data /var/lib/tuff-console
sudo -u tuff tuff console key revoke billing-ci --data /var/lib/tuff-console
```

Run the commands as the service user. A key created as `root` leaves `root`-owned files in the data folder that the service cannot write.

`create` prints the key once. The database holds only its SHA-256, so a lost key is replaced with a new one. Copy the key straight into the CI system's secret store. A key created with `--repository` publishes reports for that repository only. A key without it publishes for any repository, so give each repository or team its own.

In a container, run the same commands with `docker run --rm -v tuff-console-data:/data --entrypoint tuff tuff-console console key ... --data /data`, or with `docker exec tuff-console tuff console key list --data /data`.

## Back up

The database grows by one row for each stored report, and each row holds the report's JSON. A publish that changes only the commit, the branch, or the `dirty` flag updates the latest row, so a project that publishes on every push adds a row only when its capabilities, their status, or its policy gaps change. The console has no pruning in this release.

All state is `console.sqlite`. The database runs in WAL mode, so the folder can also hold `console.sqlite-wal` and `console.sqlite-shm` while the server runs. Copying `console.sqlite` alone while the server is running can miss recent writes.

Take a consistent copy with the SQLite shell, which works while the server runs:

```sh frame="terminal"
sudo -u tuff mkdir -p /var/backups/tuff-console
sudo -u tuff sqlite3 /var/lib/tuff-console/console.sqlite \
  ".backup '/var/backups/tuff-console/console-$(date +%F).sqlite'"
```

Alternatively, stop the service and copy the folder. A stopped server leaves only `console.sqlite`.

```sh frame="terminal"
sudo systemctl stop tuff-console
sudo cp -a /var/lib/tuff-console /var/backups/tuff-console-$(date +%F)
sudo systemctl start tuff-console
```

To restore, stop the service, put the backup at `/var/lib/tuff-console/console.sqlite`, delete any `console.sqlite-wal` and `console.sqlite-shm` next to it, make `tuff` the owner with mode `0600`, and start the service. The backup contains the SHA-256 of every API key and the full text of every report, so protect it as the database itself.

## Upgrade

Replace the `tuff` binary and restart the service. The console migrates the database when it opens it. The schema version lives in SQLite's `PRAGMA user_version`, and each migration runs once, in a transaction.

A database that a newer `tuff` has migrated is refused by an older one:

```text
error: /var/lib/tuff-console/console.sqlite has schema version 99, and this tuff reads up to 2
hint: upgrade tuff, or point --data at another folder
```

To go back to an older `tuff`, restore a backup taken before the upgrade. Take one before each upgrade.

The console is excluded from the [1.0 stability promise](/concepts/stability/), so read the [changelog](/changelog/) before upgrading.

## Try it first

`tuff console serve --demo` serves generated sample projects from memory and keeps nothing on disk. It is the quickest way to show the pages to the people who will use them before the real deployment exists.
