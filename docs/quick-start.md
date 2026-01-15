# Quick Start

Get up and running with nostr-mail in just a few steps!

## Initial Setup

### **Generate or Import Keypair**: 

   - Navigate to Settings → Nostr Settings
   - Click "Generate New Keypair" or paste your existing nsec/npriv key
   - **Important**: Save your private key securely! You'll need it to decrypt messages.

### **Configure Email**:

   - Navigate to Settings → Email Settings
   - Select your email provider (Gmail, Outlook, Yahoo, or Custom)
   - For Gmail and Yahoo:
     - Generate an app password:
        - google: https://support.google.com/accounts/answer/185833
        - yahoo: https://help.yahoo.com/kb/SLN15241.html
     - Enable IMAP in settings
   - Click "Test Email Connection" to verify settings

### **Configure Relays**:

Nostr relays are used to communicate with the nostr network. They allow you to:
    - publish your profile
    - receive incoming direct messages (DMs), which are linked to specific emails in nostr-mail
    - publish/retreive your public contacts
    - receive/publish other nostr events


When the app starts, you will automatically connect to a few nostr relays, but you may add to or remove them:

   - Navigate to Settings → Relay Settings
   - Add Nostr relays (e.g., `wss://relay.damus.io`, `wss://nostr-pub.wellorder.net`)



### **Load Contacts**:

   - Navigate to Contacts tab
   - Click "Refresh" to load contacts from your Nostr follow list
   - Contacts with email addresses will appear at the top of your contacts list with the envelope icon (✉️)
   - Click on a contact to view their profile details
   - If a contact has published their email address, you can click "Send Email" within their profile, which will you to the compose page with their information filled in.

### Compose Your First Email

**1. Navigate to Compose tab**

**2. Select Recipient**:
   - Use the dropdown to select a Nostr contact (if they have an email in their profile)
   - Or manually enter an email address in the "To:" field

**3. Write Your Message**:
   - Enter a subject line
   - Write your message in the text area

**4. Send**:
   - Click "Send" to send the encrypted email
   - By default, the message is automatically encrypted with NIP-44 and signed with your nostr key (see [Advanced settings](pages/settings.md#advanced-settings) for fine-grained control)

## Next Steps

- **Read Email**: Check the [Inbox Page](pages/inbox.md) guide
- **Send DMs**: Learn about [Direct Messages](pages/direct-messages.md)
- **Manage Contacts**: Explore the [Contacts Page](pages/contacts.md)
- **Configure Settings**: Review the [Settings Page](pages/settings.md) guide

## Tips

- Enable "Automatically Encrypt" and "Automatically Sign" in Settings for convenience
- Use multiple relays for better reliability
- Keep your private key backed up securely
- Profile pictures are cached locally for offline access
