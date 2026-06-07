# Accounts & Keys

nostr-mail supports **multiple Nostr accounts** on a single install. Your private keys are stored in the operating system's secure keychain rather than in the browser, and you can switch between identities from the account switcher in the sidebar.

## Purpose

Create, import, and switch between Nostr identities, and understand how your private keys are protected.

## Where keys are stored

Private keys are **never** kept in browser `localStorage`. They live in the OS-native secure store:

- **macOS** – Keychain
- **Windows** – Credential Manager
- **Linux** – the Secret Service / keyring backend
- **Android** – an encrypted file (Jetpack Security `EncryptedFile`) whose master key is held in the Android Keystore, inside the app-private sandbox

All of your keys are kept together in a single encrypted vault entry, keyed by public key. The backend caches the vault in memory for the session, so you are prompted by the OS keychain at most once per launch.

!!! note "Automatic migration"
    If you used an older build that stored keys in `localStorage`, nostr-mail performs a one-time migration into the keychain on first launch and then removes the plaintext copies from `localStorage`.

## Adding your first account

You can either import an existing key or generate a new one. Both are done from **Settings → Nostr Settings**:

- **Import**: paste your `nsec…` / `npriv…` private key into the *Private Key* field. The corresponding public key (`npub…`) is derived automatically.
- **Generate**: click **Generate New Keypair** to create a brand-new identity.

!!! warning "Back up your private key"
    Your private key is the only way to decrypt your messages and prove your identity. Save it somewhere safe before relying on it — there is no recovery if it is lost.

## Switching accounts

The **account switcher** lives at the bottom of the sidebar and shows your active profile (avatar, name, and short `npub`).

1. Click the active profile to open the dropdown.
2. Pick another account to switch to it instantly, **or**
3. Click **Add Account** to import or generate an additional identity.

Switching the active account changes which key is used for signing, encryption, profile publishing, and relay subscriptions. Per-account data — contacts, settings, cached profiles, emails, and DM history — is stored against the account's public key, so each identity keeps its own state.

## Key tools

In **Settings → Nostr Settings**, each key field has helper buttons:

- **Show / Hide** – reveal the private key temporarily
- **Copy** – copy the key to the clipboard
- **QR code** – display the key as a QR code (handy for moving a key to a phone)
- **Scan QR** – use the camera to scan a key in from another device

## Tips and best practices

- Treat your `nsec`/`npriv` like a password manager master key — never paste it into untrusted sites.
- Use separate accounts to keep personal and public identities apart; each has isolated contacts and settings.
- When moving to a new device, import the same private key to regain access to your encrypted history (subject to the [sync cutoff](settings.md#sync-settings)).
- Removing an account from the switcher deletes its key from the keychain on this device — make sure you have a backup first.
