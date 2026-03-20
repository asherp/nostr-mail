# Using nostr-mail with Nostr Clients

nostr-mail uses the same Nostr keypairs (nsec/npriv format) that you use with other Nostr clients. This means you can use the same identity across nostr-mail and other Nostr applications, allowing you to seamlessly bridge email and Nostr social networking.

## Compatible Clients

### Amethyst

[Amethyst](https://github.com/vitorpamplona/amethyst) is a Nostr client for Android that works well with nostr-mail. You can safely use the same private key in both applications.

**To use your Amethyst keypair with nostr-mail:**
1. In Amethyst, navigate to your account settings
2. Copy your private key (nsec format)
3. In nostr-mail, go to Settings → Nostr Settings
4. Paste your private key in the "Import Keypair" field
5. Your profile and contacts will sync automatically

### Damus

[Damus](https://damus.io/) is a popular Nostr client for iOS and macOS. While it can work with nostr-mail, there are important considerations:

**⚠️ Critical Warning: Damus Key Management**

If you plan to use your nostr-mail private key with Damus, you **must** back up your private key **before** importing it into Damus. Here's why:

1. **Profile Deletion**: When you import a private key into Damus, it will delete any existing profile associated with that key
2. **No Key Export**: Damus does not provide a way to view or export your private key after import
3. **Permanent Loss**: If you don't have a backup of your private key before importing it to Damus, you will permanently lose access to your Nostr identity

**Safe Workflow:**
1. **First**: Export/backup your private key from nostr-mail (Settings → Nostr Settings)
2. **Then**: Import the key into Damus if desired
3. **Remember**: Keep your private key backup secure - you'll need it to access your identity in other clients

**Alternative Approach:**
If you want to use Damus alongside nostr-mail, consider:
- Using separate keypairs for each application
- Or, only importing your Damus key into nostr-mail (not the other way around)

## How nostr-mail Uses Your Nostr Identity

When you use your Nostr keypair with nostr-mail, it enables several features:

### Profile Management
- Your Nostr profile (name, bio, picture) is displayed in nostr-mail
- You can update your profile from nostr-mail, and it will sync to the Nostr network
- Your profile can include your email address, making it discoverable by other nostr-mail users

### Contact Discovery
- nostr-mail automatically loads your Nostr follow list as contacts
- Contacts with email addresses in their profiles are prioritized
- You can send encrypted emails to any contact who has published their email address

### Direct Messages
- Send and receive Nostr direct messages (DMs) alongside email
- All DMs are encrypted using NIP-44 or NIP-04 encryption
- DMs are linked to email conversations when relevant

### Key Signing
- Emails can be cryptographically signed with your Nostr private key
- Recipients can verify that emails came from your Nostr identity
- This provides authentication beyond traditional email signatures

## Best Practices

1. **Backup Your Keys**: Always keep a secure backup of your private key (nsec/npriv format) before using it in multiple applications
2. **Use Multiple Relays**: Configure several Nostr relays in nostr-mail for better reliability and redundancy
3. **Profile Consistency**: Keep your email address consistent across your Nostr profile and email account for better discoverability
4. **Key Security**: Never share your private key. Only share your public key (npub format) with others

## Troubleshooting

### Profile Not Syncing
- Ensure you're connected to at least one Nostr relay
- Check that your relay settings are correct (Settings → Relay Settings)
- Try refreshing your profile (Profile tab → Refresh)

### Contacts Not Loading
- Verify your relay connections are active
- Make sure you have published a follow list on the Nostr network
- Try adding more relays for better coverage

### Key Import Issues
- Ensure you're using the correct format (nsec or npriv)
- Check that the key hasn't been corrupted (no extra spaces or line breaks)
- Verify the key works in another Nostr client before importing
