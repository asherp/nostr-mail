// DOM Manager
// Handles DOM element selection, manipulation, and utilities

class DOMManager {
    constructor() {
        this.elements = {};
        this.initializeElements();
    }

    // Initialize all DOM elements with error handling
    initializeElements() {
        this.elements = {
            // Navigation
            navItems: this.getElements('.nav-item'),
            tabContents: this.getElements('.tab-content'),
            
            // Modal
            modalOverlay: this.getElement('modal-overlay'),
            modalTitle: this.getElement('modal-title'),
            modalBody: this.getElement('modal-body'),
            modalClose: document.querySelector('.modal-close'),
            
            // Compose form
            toAddress: this.getElement('to-address'),
            subject: this.getElement('subject'),
            messageBody: this.getElement('message-body'),
            sendBtn: this.getElement('send-btn'),
            saveDraftBtn: this.getElement('save-draft-btn'),
            previewHeadersBtn: this.getElement('preview-headers-btn'),
            nostrContactSelect: this.getElement('nostr-contact-select'),
            encryptBtn: this.getElement('encrypt-btn'),
            signBtn: this.getElement('sign-btn'),
            
            // Inbox
            emailList: this.getElement('email-list'),
            refreshInbox: this.getElement('refresh-inbox'),
            emailSearch: this.getElement('email-search'),
            // Sent
            refreshSent: this.getElement('refresh-sent'),
            sentList: this.getElement('sent-list'),
            sentSearch: this.getElement('sent-search'),
            backToSent: this.getElement('back-to-sent'),
            sentDetailView: this.getElement('sent-detail-view'),
            sentDetailContent: this.getElement('sent-detail-content'),
            sentActions: this.getElement('sent-actions'),
            sentTitle: this.getElement('sent-title'),
            
            // Drafts
            refreshDrafts: this.getElement('refresh-drafts'),
            draftsList: this.getElement('drafts-list'),
            backToDrafts: this.getElement('back-to-drafts'),
            draftsDetailView: this.getElement('drafts-detail-view'),
            draftsDetailContent: this.getElement('drafts-detail-content'),
            draftsActions: this.getElement('drafts-actions'),
            draftsTitle: this.getElement('drafts-title'),
            
            // DM elements (only static ones)
            dmContacts: this.getElement('dm-contacts'),
            dmMessages: this.getElement('dm-messages'),
            refreshDm: this.getElement('refresh-dm'),
            dmSearch: this.getElement('dm-search'),
            dmSearchToggle: this.getElement('dm-search-toggle'),
            dmSearchContainer: this.getElement('dm-search-container'),
            
            // Contacts
            contactsList: this.getElement('contacts-list'),
            addContactBtn: this.getElement('add-contact-btn'),
            refreshContactsBtn: this.getElement('refresh-contacts-btn'),
            contactsDetail: this.getElement('contacts-detail'),
            contactsSearch: this.getElement('contacts-search'),
            contactsSearchToggle: this.getElement('contacts-search-toggle'),
            contactsSearchContainer: this.getElement('contacts-search-container'),
            
            // Profile (do not look for display-name, about, email, nip05)
            updateProfileBtn: this.getElement('update-profile-btn'),
            refreshProfile: this.getElement('refresh-profile'),
            
            // Relays
            relaysList: this.getElement('relay-list'),
            relaySummary: this.getElement('relay-summary'),
            
            // Live Events
            liveEventsIndicator: this.getElement('live-events-indicator'),
            liveEventsText: this.getElement('live-events-text'),
            newRelayUrl: this.getElement('new-relay-url'),
            addRelayBtn: this.getElement('add-relay-btn'),
            
            // Settings
            nprivKey: this.getElement('npriv-key'),
            generateKeyBtn: this.getElement('generate-key-btn'),
            publicKeyDisplay: this.getElement('public-key-display'),
            copyPubkeyBtn: this.getElement('copy-pubkey-btn'),
            encryptionAlgorithm: this.getElement('encryption-algorithm'),
            emailProvider: this.getElement('email-provider'),
            emailAddress: this.getElement('email-address'),
            emailPassword: this.getElement('email-password'),
            smtpHost: this.getElement('smtp-host'),
            smtpPort: this.getElement('smtp-port'),
            imapHost: this.getElement('imap-host'),
            imapPort: this.getElement('imap-port'),
            'use-tls': this.getElement('use-tls'),
            emailFilterPreference: this.getElement('email-filter-preference'),
            'send-matching-dm-preference': this.getElement('send-matching-dm-preference'),
            'require-signature-preference': this.getElement('require-signature-preference'),
            'hide-undecryptable-emails-preference': this.getElement('hide-undecryptable-emails-preference'),
            'automatically-encrypt-preference': this.getElement('automatically-encrypt-preference'),
            'automatically-sign-preference': this.getElement('automatically-sign-preference'),
            'hide-unsigned-messages-preference': this.getElement('hide-unsigned-messages-preference'),
            syncCutoffDays: this.getElement('sync-cutoff-days'),
            emailsPerPage: this.getElement('emails-per-page'),
            testEmailConnectionBtn: this.getElement('test-email-connection-btn'),
            copyNprivBtn: this.getElement('copy-npriv-btn'),
            copyEmailPasswordBtn: this.getElement('copy-email-password-btn'),
            toggleNprivVisibilityBtn: this.getElement('toggle-npriv-visibility-btn'),
            toggleEmailPasswordVisibilityBtn: this.getElement('toggle-email-password-visibility-btn'),
            qrNprivBtn: this.getElement('qr-npriv-btn'),
            qrNpubBtn: this.getElement('qr-npub-btn'),
        };
    }

    // Get element by ID with error handling
    getElement(id) {
        const element = document.getElementById(id);
        if (!element) {
            console.warn(`Element with id '${id}' not found`);
        }
        return element;
    }

    // Get elements by selector with error handling
    getElements(selector) {
        const elements = document.querySelectorAll(selector);
        if (elements.length === 0) {
            console.warn(`No elements found for selector '${selector}'`);
        }
        return elements;
    }

    // Get element by key
    get(key) {
        return this.elements[key];
    }

    // Set element value
    setValue(key, value) {
        const element = this.elements[key];
        if (element) {
            element.value = value;
        }
    }

    // Get element value
    getValue(key) {
        const element = this.elements[key];
        return element ? element.value : '';
    }

    // Get checkbox checked state
    isChecked(key) {
        const element = this.elements[key];
        return element ? element.checked : false;
    }

    // Set element text content
    setText(key, text) {
        const element = this.elements[key];
        if (element) {
            element.textContent = text;
        }
    }

    // Set element inner HTML
    setHTML(key, html) {
        const element = this.elements[key];
        if (element) {
            element.innerHTML = html;
        }
    }

    // Add event listener
    addEventListener(key, event, handler) {
        const element = this.elements[key];
        if (element) {
            element.addEventListener(event, handler);
        }
    }

    // Remove event listener
    removeEventListener(key, event, handler) {
        const element = this.elements[key];
        if (element) {
            element.removeEventListener(event, handler);
        }
    }

    // Show/hide element
    show(key) {
        const element = this.elements[key];
        if (element) {
            element.style.display = '';
        }
    }

    hide(key) {
        const element = this.elements[key];
        if (element) {
            element.style.display = 'none';
        }
    }

    // Add/remove CSS classes
    addClass(key, className) {
        const element = this.elements[key];
        if (element) {
            element.classList.add(className);
        }
    }

    removeClass(key, className) {
        const element = this.elements[key];
        if (element) {
            element.classList.remove(className);
        }
    }

    toggleClass(key, className) {
        const element = this.elements[key];
        if (element) {
            element.classList.toggle(className);
        }
    }

    // Enable/disable element
    enable(key) {
        const element = this.elements[key];
        if (element) {
            element.disabled = false;
        }
    }

    disable(key) {
        const element = this.elements[key];
        if (element) {
            element.disabled = true;
        }
    }

    // Focus element
    focus(key) {
        const element = this.elements[key];
        if (element) {
            element.focus();
        }
    }

    // Clear element value
    clear(key) {
        const element = this.elements[key];
        if (element) {
            element.value = '';
        }
    }
}

// Create and export a singleton instance
window.DOMManager = DOMManager; 