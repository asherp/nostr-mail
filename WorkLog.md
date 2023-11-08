
# 2023-11-08 09:49:26.726445: clock-in

# 2023-11-08 09:17:24.479631: clock-out

* loading dms


# 2023-11-08 08:27:21.298092: clock-in

# 2023-11-07 16:58:54.524724: clock-out

* issue: confirming save - this isn't working
* replaceable event discussion (for mailing list):
	- use list of moderators
	- subscribers are in list of [(hash, encrypted asymmetric key)]
	- mods are in list of [(mod pub key)]
	- subscription list are published by mods independently
	- subscribers follow the mods
* any mod can update the subscriber list
* other mods republish each other's changes

# 2023-11-07 13:28:39.328472: clock-in

# 2023-11-07 09:41:10.698492: clock-out

* saving profile
* refactor layout, fail gracefully when offline

# 2023-11-07 08:29:58.746735: clock-in

# 2023-11-06 22:57:41.595968: clock-out

* refactor with relay Manger

# 2023-11-06 21:49:34.478942: clock-in

# 2023-11-06 14:02:32.612313: clock-out

* setting up profile save

# 2023-11-06 13:33:18.272176: clock-in

# 2023-11-06 10:03:05.841786: clock-out

* refactor, load profile, async

# 2023-11-06 07:28:11.225384: clock-in

# 2023-11-04 23:24:22.935479: clock-out

* working relay add, remove

# 2023-11-04 22:03:35.534363: clock-in

# 2023-11-04 19:18:37.993656: clock-out

* save, load relays

# 2023-11-04 18:11:42.384464: clock-in: T-10m 

# 2023-11-04 16:17:29.788581: clock-out: T-15m 
* profile loading. look at using  `aionostr.relay.RelayPool`

# 2023-11-04 15:14:07.577390: clock-in

# 2023-11-03 19:47:54.771049: clock-out: T-20m 

* testing profile loading into db

# 2023-11-03 18:01:35.470392: clock-in

# 2023-11-02 16:24:28.645786: clock-out

* looking at aoinostr api

# 2023-11-02 15:35:58.694842: clock-in: T-4m 

# 2023-11-01 19:52:24.403884: clock-out

* save, load email credentials

# 2023-11-01 18:58:43.427874: clock-in

# 2023-11-01 16:33:49.770381: clock-out

* save, load nostr priv, pub key
* aionostr docs https://github.com/davestgermain/aionostr/blob/master/docs/usage.rst

# 2023-11-01 15:02:03.890320: clock-in

# 2023-10-31 15:40:58.074889: clock-out

* nostr dependencies

# 2023-10-31 14:37:20.178561: clock-in

# 2023-10-31 08:43:57.102223: clock-out

* key storage
* nostr private key storage https://www.reddit.com/r/nostr/comments/10thhgh/managing_nostr_private_key/

# 2023-10-31 07:52:33.894620: clock-in: T-4m 

# 2023-10-30 20:12:34.177188: clock-out

* nav, compose, inbox, settings layout

# 2023-10-30 18:00:46.852777: clock-in

# 2023-10-30 10:33:01.810709: clock-out

* kivy app

# 2023-10-30 08:29:41.131573: clock-in

# 2023-04-29 01:11:50.217567: clock-out

* documentation, presentation

# 2023-04-28 23:06:24.151223: clock-in

# 2023-04-28 18:01:30.320739: clock-out

* packaging, discussion with Tadge
* Tadgh Drija on cc'ing: use a single encryption key and encrypt it to everyone's pub keys and include the whole decryption package in the message. This could be a json blob storing the following

```yaml
scheme: aes256
receiver keys:
- pub_key: encrypted decryption key
- ...
```
	
# 2023-04-28 17:55:10.766292: clock-in

# 2023-04-28 12:05:40.850929: clock-out

* docs

# 2023-04-28 10:28:11.651768: clock-in

# 2023-04-23 15:08:29.738699: clock-out

* handle imap connection error

# 2023-04-23 14:53:12.076305: clock-in

# 2023-04-23 10:17:54.445850: clock-out

* caching improvements
looking at session storage

# 2023-04-23 09:19:40.385017: clock-in

# 2023-04-19 23:43:27.964016: clock-out

* improved loading times to avoid timeout

# 2023-04-19 23:03:12.254233: clock-in

# 2023-04-15 10:57:38.662251: clock-out

* refreshing user profile

# 2023-04-15 09:59:33.579529: clock-in

# 2023-04-15 09:49:01.574530: clock-out

* cache reset button

# 2023-04-15 09:15:32.803650: clock-in

# 2023-04-10 22:48:09.284772: clock-out

* testing user container

# 2023-04-10 22:05:43.542150: clock-in


* to sanize emails, check out amonia with nh3 python binding https://nh3.readthedocs.io/en/latest/

# 2023-04-09 23:51:00.895163: clock-out

* update profile button

# 2023-04-09 22:08:51.287745: clock-in

# 2023-04-09 10:25:57.371034: clock-out

* hot reload by env, edit user profile

# 2023-04-09 10:11:08.859105: clock-in

# 2023-04-08 19:18:40.274196: clock-out

* documentation

# 2023-04-08 18:12:51.398424: clock-in

* example env file
# 2023-04-08 13:44:35.437850: clock-out

* got email working with dms

# 2023-04-08 10:45:50.749441: clock-in

# 2023-03-26 16:14:00.530769: clock-out

* rendering avatars in inbox

# 2023-03-26 15:17:45.151187: clock-in

# 2023-03-21 14:19:28.772712: clock-out

* conversations

# 2023-03-21 13:06:06.735484: clock-in

# 2023-03-20 19:09:45.883040: clock-out: T-10m 

* update inbox, avatars

# 2023-03-20 18:11:56.948100: clock-in

# 2023-03-20 11:05:01.507585: clock-out

* fetching dms

# 2023-03-20 10:14:38.822856: clock-in

# 2023-03-19 23:45:45.312979: clock-out


# 2023-03-19 22:38:39.007652: clock-in

# 2023-03-19 21:38:20.532966: clock-out

* setting up dms

# 2023-03-19 21:10:30.939365: clock-in

# 2023-03-19 02:04:02.230490: clock-out

* email send

# 2023-03-18 23:39:36.131727: clock-in

* ignoring env

# 2023-03-18 22:07:31.239372: clock-out

* encrypting subject and body to receiver

# 2023-03-18 20:23:13.413597: clock-in

# 2023-03-18 18:56:29.590170: clock-out

* render user profile, credentials

# 2023-03-18 17:43:22.541161: clock-in

# 2023-03-17 22:26:16.589981: clock-out: T-30m 

* alice, bob services

# 2023-03-17 21:08:16.620064: clock-in

# 2023-03-12 22:16:03.525915: clock-out

* fetching email, profile pics

# 2023-03-12 21:28:32.683363: clock-in

# 2023-03-12 19:07:52.308489: clock-out

* imap receiver

# 2023-03-12 18:52:17.463282: clock-in

# 2023-03-11 22:37:58.222838: clock-out

* publish alice and bob profiles

# 2023-03-11 22:08:11.340599: clock-in

# 2023-03-11 20:40:25.042590: clock-out

* alice and bob keys

# 2023-03-11 19:47:55.859711: clock-in

# 2023-03-11 17:59:30.794963: clock-out

* rendering contacts profile

# 2023-03-11 15:37:57.782640: clock-in

# 2023-03-11 14:45:05.108228: clock-out

* switch from Fernet to nostr encryption scheme

# 2023-03-11 13:28:19.085627: clock-in

# 2023-03-05 20:07:43.617401: clock-out

* setting up container priv keys

# 2023-03-05 19:13:13.083170: clock-in

# 2023-03-01 20:53:56.066092: clock-out

* adding credentials, settings
* sidebar example https://dash-bootstrap-components.opensource.faculty.ai/examples/simple-sidebar/
* configuring smtp https://red-mail.readthedocs.io/en/stable/tutorials/client.html#config-smtp

# 2023-03-01 19:12:17.414119: clock-in

# 2023-02-27 18:30:14.616845: clock-out

* reading email in python https://www.thepythoncode.com/article/reading-emails-in-python

# 2023-02-27 18:24:34.923804: clock-in

# 2023-02-26 21:56:20.776792: clock-out

* installable package, basic email gui

# 2023-02-26 20:41:38.192299: clock-in

# 2023-02-25 13:29:56.840093: clock-out

* working symmetric TOTP encryption

# 2023-02-25 11:55:02.107526: clock-in

# 2023-02-25 11:31:27.280425: clock-out

* nip05 p2p registry

# 2023-02-25 10:54:24.729659: clock-in

# 2023-02-16 16:28:37.548094: clock-out

* set up dependencies, trying nostr queries

# 2023-02-16 14:35:04.909245: clock-in

