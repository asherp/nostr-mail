# Contacts Page

The Contacts page displays and manages your Nostr contacts.

## Purpose

View, search, and interact with your Nostr contacts.

## Key Features

- **Contact List**: View all contacts from your Nostr follow list
- **Profile Details**: View complete contact profiles with metadata
- **Contact Actions**: Send emails, DMs, or copy public keys
- **Privacy Management**: Change contact privacy status via profile view
- **Search**: Search contacts by name
- **Add Contacts**: Manually add contacts by public key
- **Export Contacts**: Export contact list as npub list
- **Progressive Loading**: Images load progressively for better performance

## Usage Instructions

### 1. View Contacts

- Navigate to Contacts tab
- View list of all contacts sorted alphabetically
- Click on a contact to view profile details

### 2. View Profile Details

- Click on any contact in the list
- View full profile including name, picture, about, email, and public key
- See all available profile metadata

### 3. Contact Actions

- **Send Email**: Click "Send Email" to compose an email to this contact
- **Send DM**: Click "Send DM" to start a direct message conversation
- **Copy Pubkey**: Click "Copy Pubkey" to copy the contact's public key

### 4. Search Contacts

- Click "Search" button
- Type to filter contacts by name

### 5. Add Contact

- Click "Add Contact" button
- Enter a public key (npub format) or scan QR code
- Contact will be added to your list

### 6. Export Contacts

- Click "Export Contacts" to download a list of all npubs

### 7. Refresh

- Click "Refresh" to sync contacts from published kind 3 (follow list) events on Nostr relays
- Contacts found in your published follow list are marked as public
- Contacts not in your published follow list are marked as private
- If no kind 3 event is found on any relay, sync is aborted and existing contacts are preserved (check your relay configuration)
- If a kind 3 event is found but contains 0 follows (empty list), sync is also aborted to preserve existing contacts
- Uses the latest kind 3 event from any responding relay

## Public vs Private Contacts

Contacts can be either **public** or **private**:

- **Public contacts**: Appear in your published kind 3 (follow list) event on relays. These are visible to others who view your public follow list.
- **Private contacts**: Stored locally but not published to relays. These contacts are only visible to you and won't appear in your public follow list.

The privacy status determines whether a contact appears in your published follow list when syncing with relays.

## Managing Contact Privacy

Privacy status can only be changed when viewing a contact's profile:

1. Click on a contact in the contacts list to view their profile
2. Use the privacy toggle in the profile view to change between public and private
3. Changes are saved to the database immediately
4. Privacy changes are automatically published to relays - When you toggle a contact to public or private, the updated follow list (kind 3 event) is immediately published to all active relays. If publishing fails, you'll see a warning notification and the change will remain local only

## Tips and Best Practices

- Contacts are automatically loaded from your Nostr follow list
- Profile pictures are cached locally for offline access
- Only contacts with email addresses appear in the Compose page dropdown
- Use search to quickly find specific contacts
- Export contacts to backup your contact list
- Manually add contacts that aren't in your follow list
- Ensure relays are configured correctly for contact sync to work properly
- Contacts sync prioritizes published kind 3 events from relays