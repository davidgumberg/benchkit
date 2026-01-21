# Security

Given that a benchkit client will run arbitrary code that is published on the
NATS channel that it is subscribed to, the damage that can be done if there is a
vulnerability or an error in the setup is maximal.

Do not run networked clients on devices that have access to valuable data or
infrastructure, they should either be run in virtual machines or on dedicated
benchmarking hardware that is isolated from other devices on your network, not
on personal devices!

The security of this deployment relies on:

1. The client authenticating the identity of the server via TLS certificate,
   which is either signed by a publicly trusted root Certificate Authority, or
   self-signed by the operator and the Certificate Authority `.crt` is
   transmitted out of band to the client.
   - If an attacker gets the private key of a self-signing certificate authority
     that is passed to BenchKit or of one of the signed certificates, they can
     execute arbitrary code on clients.

2. The nats-server being set up to only allow one or some authenticated users to
   publish.
   - If an attacker gets the credentials for any of the privileged users, they
     can execute arbitrary code on clients. The credentials should be strong
     (use NKeys) and kept secret.

## Authenticated NATS

Don't user user/password authentication, use nkeys instead.

```bash
# Acquire nkeys
# maybe: go install github.com/nats-io/nkeys/nk@latest

# Generate a user key, store it in nk.key.
nk -gen user > nk.key

# Get the associated pubkey.
nk -inkey nk.key -pubout > nk.pub
```

Create a nats.conf file that gives the authenticated user permissions to
publish, and everyone else only gets subscribe permissions.

```bash
cat <<EOF > nats.conf
authorization {
    users =
    [
        {
            # No usernames for nkeys.
            nkey: "$(cat nk.pub)"
            permissions: {
                publish: { allow: "benchkit.jobs" },
                subscribe: { allow: "benchkit.jobs" }
            }
        },
        {
            user: "listener"
            # Optionally, no nkey or password for listener.
            # Note that this makes it possible for anyone to listen in on
            # benchkit.jobs, which is probably harmless.
            permissions: {
                publish: { deny: ">" },
                subscribe: { allow: "benchkit.jobs" }
            }
        }
    ]
}

# Default user when client connects with no authentication.
# Don't include this if using authentication for listener user.
no_auth_user: "listener"

EOF
```

## TLS Certificate

```bash
# Create a certificate authority:
openssl req -x509 -noenc -subj "/CN=BenchkitCA" \
    -keyout ca.key -out ca.crt

# Create a certificate request:
openssl req -new -noenc -subj "/CN=BenchkitServ" \
  -keyout server.key -out server.csr

# Sign it, changing the IP:{address} and DNS:{hostname} accordingly:
openssl x509 -req -in server.csr -CA ca.crt -CAkey ca.key -CAcreateserial \
  -extfile <(printf "subjectAltName=IP:127.0.0.1,DNS:localhost") \
  -out server.crt

# Recommended, delete the certificate authority key:
rm ca.key server.csr

# Optionally, write the cert and key paths to nats.conf, this makes assumptions
# about the file layout you are using, but feel free to modify this, or omit
# this and pass the cert/key paths as arguments to nats-server.
cat <<EOF >> nats.conf
tls {
    cert_file: "server.crt",
    key_file: "server.key"
}

EOF
```


## Pre-flight check

These instructions assume you are using self-signed certificates, and that your
nats server is running on localhost at the default port, but you can easily
permute them to suit a setup with root-signed certificates and with a remote
nats-server if those assumptions don't apply.

Start up the server in one terminal window:

```bash
nats-server -c nats.conf
```

In another terminal window, from the same directory as your server config and
certificates, make sure you get the same output for these commands as shown
here:

### Certificates

```console
$ nats sub benchkit.jobs
nats: error: tls: failed to verify certificate: x509: certificate signed by unknown authority
```

### Subscriptions

The anonymous user has permission to subscribe to `benchkit.jobs`:
```console
$ nats --tlsca=ca.crt sub benchkit.jobs --wait=1s
12:12:49 Subscribing on benchkit.jobs
```

But not other channels:

```console
$ nats --tlsca=ca.crt sub hello --wait=1s
Unexpected NATS error from server nats://127.0.0.1:4222: nats: permissions violation: Permissions Violation for Subscription to "hello"
nats: error: nats: permissions violation: Permissions Violation for Subscription to "hello"
```

### Publishing

The anonymous user can't publish:

```console
$ nats --tlsca=ca.crt pub hello "world"
Unexpected NATS error from server nats://127.0.0.1:4222: nats: permissions violation: Permissions Violation for Publish to "hello"
nats: error: nats: permissions violation: Permissions Violation for Publish to "hello"
```

```console
$ nats --tlsca=ca.crt pub benchkit.jobs "hello world"
Unexpected NATS error from server nats://127.0.0.1:4222: nats: permissions violation: Permissions Violation for Publish to "benchkit.jobs"
nats: error: nats: permissions violation: Permissions Violation for Publish to "benchkit.jobs"
```

But the authorized user can publish to benchkit.jobs:

```console
$ nats --tlsca=ca.crt --nkey=nk.key pub benchkit.jobs "hello world"
Published 11 bytes to "benchkit.jobs"
```

But not elsewhere:

```console
$ nats --tlsca=ca.crt --nkey=nk.key pub hello "world"
Unexpected NATS error from server nats://127.0.0.1:4222: nats: permissions violation: Permissions Violation for Publish to "hello"
nats: error: nats: permissions violation: Permissions Violation for Publish to "hello"
```


