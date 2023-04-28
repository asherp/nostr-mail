
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



## Usage

Navigate to the base of this directory, then:

```sh
docker compose up nostrmail
```

Navigate to [http://localhost:8050](http://localhost:8050)


### Requirements

* secp256k1 https://pypi.org/project/secp256k1/
* nostr https://pypi.org/project/nostr/


