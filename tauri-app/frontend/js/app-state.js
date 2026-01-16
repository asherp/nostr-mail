// App State Management
// Handles all application state and provides a centralized state management system

class AppState {
    constructor() {
        this.currentUser = null;
        this.contacts = [];
        this.emails = [];
        this.sentEmails = [];
        this.settings = null;
        this.keypair = null;
        this.dmContacts = [];
        this.dmMessages = {};
        this.selectedDmContact = null;
        this.selectedContact = null;
        this.nprivKey = null;
        this.relays = [];
        this.contacts_total = null;
        this.lastLoadedPubkey = null;
    }

    // Contact management
    setContacts(contacts) {
        this.contacts = contacts;
    }

    getContacts() {
        return this.contacts;
    }

    addContact(contact) {
        this.contacts.push(contact);
        this.sortContacts();
    }

    updateContact(pubkey, updates) {
        const index = this.contacts.findIndex(c => c.pubkey === pubkey);
        if (index !== -1) {
            this.contacts[index] = { ...this.contacts[index], ...updates };
        }
    }

    sortContacts() {
        this.contacts.sort((a, b) => {
            const nameA = a.name.toLowerCase();
            const nameB = b.name.toLowerCase();
            return nameA.localeCompare(nameB);
        });
    }

    // DM management
    setDmContacts(contacts) {
        this.dmContacts = contacts;
    }

    getDmContacts() {
        return this.dmContacts;
    }

    addDmContact(contact) {
        if (!this.dmContacts.find(c => c.pubkey === contact.pubkey)) {
            this.dmContacts.push(contact);
        }
    }

    setDmMessages(contactPubkey, messages) {
        this.dmMessages[contactPubkey] = messages;
    }

    getDmMessages(contactPubkey) {
        return this.dmMessages[contactPubkey] || [];
    }

    addDmMessage(contactPubkey, message) {
        if (!this.dmMessages[contactPubkey]) {
            this.dmMessages[contactPubkey] = [];
        }
        this.dmMessages[contactPubkey].push(message);
    }

    // Email management
    setEmails(emails) {
        this.emails = emails;
    }

    getEmails() {
        return this.emails;
    }

    setSentEmails(emails) {
        this.sentEmails = emails;
    }
    getSentEmails() {
        return this.sentEmails || [];
    }

    // Drafts management
    setDrafts(drafts) {
        this.drafts = drafts;
    }

    getDrafts() {
        return this.drafts || [];
    }

    // Settings management
    setSettings(settings) {
        this.settings = settings;
    }

    getSettings() {
        return this.settings;
    }

    // Keypair management
    setKeypair(keypair) {
        this.keypair = keypair;
        this.nprivKey = keypair?.private_key || null;
    }

    getKeypair() {
        return this.keypair;
    }

    setLastLoadedPubkey(pubkey) {
        this.lastLoadedPubkey = pubkey;
    }

    getLastLoadedPubkey() {
        return this.lastLoadedPubkey;
    }

    setNprivKey(nprivKey) {
        this.nprivKey = nprivKey;
    }

    getNprivKey() {
        return this.nprivKey;
    }

    // Relay management
    setRelays(relays) {
        this.relays = relays;
    }

    getRelays() {
        return this.relays;
    }

    getActiveRelays() {
        return this.relays.filter(r => r.is_active).map(r => r.url);
    }

    // Selection management
    setSelectedContact(contact) {
        this.selectedContact = contact;
    }

    getSelectedContact() {
        return this.selectedContact;
    }

    setSelectedDmContact(contact) {
        this.selectedDmContact = contact;
    }

    getSelectedDmContact() {
        return this.selectedDmContact;
    }

    // Utility methods
    hasKeypair() {
        return !!this.keypair;
    }

    hasSettings() {
        return !!this.settings;
    }

    hasEmailSettingsConfigured() {
        if (!this.settings) {
            return false;
        }
        // Check if required email settings are configured
        const emailAddress = this.settings.email_address?.trim();
        const password = this.settings.password?.trim();
        const smtpHost = this.settings.smtp_host?.trim();
        
        return !!(emailAddress && password && smtpHost);
    }

    hasActiveRelays() {
        return this.getActiveRelays().length > 0;
    }
}

// Create and export a singleton instance
window.AppState = AppState;
window.appState = new AppState(); 