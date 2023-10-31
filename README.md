
## NostrMail

A simple email encryption tool based on secp256 key pairs.

How it works:

NostrMail encrypts content using a symetric key derived from a combination of the sender's private key and the receiver's public key.

Both sender and receiver derive a shared secret known only to them, which is used to protect their communications.

This application can use any email server for delivery.

### Why have you done this?

NostrMail aims to improve privacy for the average pleb by bridging the gap between nostr and email. The two protocols serve different purposes, but they also solve each other's problems. For example, PGP does exist for email but it has not seen mainstream adoption because it relies on an existing key registry.

| Feature            | Nostr                               | Email                             | NostrMail                  |
| -------------------|-------------------------------------| --------------------------------- |--------------------------- |
| Social Key Registry| :material-checkbox-marked:          | :material-checkbox-blank-outline: | :material-checkbox-marked: |
| PGP                | :material-checkbox-marked:          | :material-checkbox-marked:        | :material-checkbox-marked: |
| Long form content  | :material-checkbox-blank-outline:   | :material-checkbox-marked:        | :material-checkbox-marked: |
| Archival Storage   | :material-checkbox-blank-outline:   | :material-checkbox-marked:        | :material-checkbox-marked: |
| Ubiquitous         | :material-checkbox-blank-outline:   | :material-checkbox-marked:        | :material-checkbox-marked: |

### Obligatory warning

Nostrmail uses NIP-04, which has many [issues pointed out here](https://github.com/nostr-protocol/nips/issues/107). While not perfect, it's better than cleartext. Use at your own risk.

## App

### Desktop

Use conda/miniconda to install dependencies

```sh
conda create -n nostrmail python=3.9
conda activate nostrmail
pip install -r app/requirements.txt
```




## Usage

You'll need [docker](https://docs.docker.com/desktop/).

Clone and navigate to the base of the repo directory, then:

```sh
docker compose up nostrmail
```

Navigate to [http://localhost:8050](http://localhost:8050)

Here are all the services you can run with `docker compose up <service name>`

| service | purpose | port |
| --------|---------|------|
| nostrmail | main dashboard site | 8050 | 
| alice   | "Alice" dashboard for testing | 8051 |
| bob     | "Bob" dashboard for testing | 8052 |
| docs    | documentation site | 8000 |
| notebook | jupyter notebook for prototyping | 8888 |


## Docker Configuration

### Environment variables


Create a `.env` file and place it in the base of this repo to set the defaults for the above containers.


```sh

{! ../.env.example !}

```

### Address book/relays

Create a file in the local directory called `address_book.yaml` to specify private contacts.
Here's an example.

```yaml
contacts:
  - username: alice
    pubkey: 12697aa72d2269aa632319d000b0548235d1d385dc16260ca77f704e802b5483
  - username: bob
    pubkey: 8619149c5549fa9970c042da77d9d018c7213e83aa49b89c234da9c298ecb941
  - username: asher
    pubkey: 86fb0bd1f7edcb17b39e897488f51f1d22ac6bd93aae491fc7cd45c9fb0d4ad8
relays:
  - wss://nostr-pub.wellorder.net
  - wss://relay.damus.io
```

### Email

Configure your email account to allow sending and receiving emails. Here are instructions for gmail.

1. Generate an app password (required if using 2-factor auth) See https://support.google.com/accounts/answer/185833?hl=en 
1. Set `EMAIL_PASSWORD` in your `.env` file as explained above.
1. Open gmail settings to enable IMAP
    1. In the top right, click Settings Settings and then See all  settings.
    1. Click the Forwarding and POP/IMAP tab.
    1. In the "IMAP access" section, select Enable IMAP.
    1. Click Save Changes.


