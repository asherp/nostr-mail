## NIP-??

This NIP defines the basic requirements that should be implemented by anyone wanting to support email alongside nostr.

### Motivation

Email integration enables several features for nostr users that relays alone cannot.

#### Long form messaging

Nostr messages are intended to mimic the short form messaging of social media. It is largely up to the relay to determine the length of messages, but attachments and formatting are not supported (other NIPs that address this?). Email provides a means of communicating much larger messages, which are more suitable in personal or business contxts (with the added bonus of privacy offered by nostr's encryption).

#### Archival storage

Nostr relays are not required to store DMs permanently. With nostr-mail DMs are replicated (in encrypted form) as the subject of an associated email. Thus, email provides a free back up for any DMs sent or received in this manner, in addition to any longer form content or files intended for long term storage.

#### Email authentication/identification

While the [NIP-05](https://github.com/nostr-protocol/nips/blob/master/05.md) standard allows nostr profiles to be associated with a user who controls their own domain, this is not the case for vast majority of users. Nostr-mail provides a more accessible way to associate one's identity with a public key. The setup process will be familiar to anyone who uses a desktop email client: simply allow the email server to accept SMTP connections and provide email credentials to the nostr-mail client.

#### Email privacy

While Email PGP has been availble in various forms since 1991. Despite decades of attempts to educate the public, it has not seen wide adoption. Since all Nostr users have key pairs by default, we can leap frog the education propblems associated with traditional PGP. [NIP-04](https://github.com/nostr-protocol/nips/blob/master/04.md) provides a mechanism for encrypting DMs, and we apply the scheme to encrypt/decrypt emails as well as link them to specific Nostr DMs.

#### Key rotation

In the event that a user loses access to their private key, Email provides an out-of-band means of communicating a new one. A more formal key rotation mechanism that utilizes email is outside the scope of this NIP, but we hope to address it further in the future!

### Email signaling

`final` `optional` `author:asherp` `author:asherp`

A special `email` field in the user's profile is all that is required to signal that the user **accepts** encrypted email.

Here is example python code that accomplishes this, using [python-nostr](https://github.com/jeffthibault/python-nostr)

```python
from nostr.relay_manager import RelayManager
from nostr.key import PrivateKey
from nostr.filter import Filter, Filters
from nostr.event import Event, EventKind
import time

def publish_profile(priv_key, profile_dict):
    relay_manager = RelayManager()

    for relay in relays:
        relay_manager.add_relay(relay)
    relay_manager.open_connections({"cert_reqs": ssl.CERT_NONE}) # NOTE: This disables ssl certificate verification
    print('waiting 1.5 sec to open')
    time.sleep(1.5)
    
    event_profile = Event(priv_key.public_key.hex(),
                          json.dumps(profile_dict),
                          kind=EventKind.SET_METADATA)
    priv_key.sign_event(event_profile)
    
    # check signature
    assert event_profile.verify()
    relay_manager.publish_event(event_profile)
    print('waiting 1 sec to send')
    time.sleep(1) # allow the messages to send

    relay_manager.close_connections()
    return event_profile.signature

alice_profile = dict(display_name='Alice',
              name='alice',
              picture='https://encrypted-tbn0.gstatic.com/images?q=tbn:ANd9GcTV-0rZbgnQcRbDqbk0hHPLHPyHpqLJ8xkriA&usqp=CAU',
              about='my name is Alice..',
              email='alice@tld.com')

```

### DM replication

When an email is sent from a nostr-mail client, two things **must** occur:

1. a DM with the encrypted subject of the email should be broadcast to the user's nostr relays
1. the body of the email **must** be encrypted with the same shared secret.
1. an email with the encrypted subject and body (using the same shared secret) **must** be sent via the user's SMTP server

It is crucial that the email subject matches the encrypted DM exactly. This allows the receiver to verify that the email came from the same author as the DM - specifically, the author who's private key signed the DM event. Second, it allows the receiver to find the encrypted message on their mail server using the subject. Specifically, the `iv` string used in the encryption will be unique to that message.
