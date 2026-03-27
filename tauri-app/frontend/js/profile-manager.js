// Profile Manager
// Manages multiple nostr accounts/profiles, allowing switching without re-entering keys

class ProfileManager {
    constructor() {
        this.ACCOUNTS_KEY = 'nostr_mail_accounts';
        this.ACTIVE_KEY = 'nostr_keypair';
        this.PROFILES_CACHE_KEY = 'nostr_mail_profiles';
    }

    // One-time migration from single-keypair storage to multi-account array
    migrateFromSingleKeypair() {
        if (localStorage.getItem(this.ACCOUNTS_KEY)) {
            return; // already migrated
        }
        const existing = localStorage.getItem(this.ACTIVE_KEY);
        if (existing) {
            try {
                const kp = JSON.parse(existing);
                if (kp && kp.private_key && kp.public_key) {
                    const accounts = [{
                        public_key: kp.public_key,
                        private_key: kp.private_key,
                        label: '',
                        added_at: new Date().toISOString()
                    }];
                    this.saveAccounts(accounts);
                    console.log('[ProfileManager] Migrated single keypair to accounts list');
                }
            } catch (e) {
                console.error('[ProfileManager] Migration failed:', e);
            }
        }
    }

    getAccounts() {
        try {
            const raw = localStorage.getItem(this.ACCOUNTS_KEY);
            return raw ? JSON.parse(raw) : [];
        } catch (e) {
            console.error('[ProfileManager] Failed to read accounts:', e);
            return [];
        }
    }

    saveAccounts(accounts) {
        localStorage.setItem(this.ACCOUNTS_KEY, JSON.stringify(accounts));
    }

    getActiveAccount() {
        try {
            const raw = localStorage.getItem(this.ACTIVE_KEY);
            return raw ? JSON.parse(raw) : null;
        } catch (e) {
            return null;
        }
    }

    // Add a new account. Returns the account object, or null if invalid/duplicate.
    async addAccount(privateKey, label) {
        if (!Utils.isValidNostrPrivkey(privateKey)) {
            console.warn('[ProfileManager] Invalid private key format');
            return null;
        }

        let publicKey;
        try {
            const valid = await TauriService.validatePrivateKey(privateKey);
            if (!valid) return null;
            publicKey = await TauriService.getPublicKeyFromPrivate(privateKey);
        } catch (e) {
            console.error('[ProfileManager] Key validation failed:', e);
            return null;
        }

        const accounts = this.getAccounts();
        const existing = accounts.find(a => a.public_key === publicKey);
        if (existing) {
            // Update private key and label if provided (in case format changed)
            existing.private_key = privateKey;
            if (label) existing.label = label;
            this.saveAccounts(accounts);
            return existing;
        }

        const account = {
            public_key: publicKey,
            private_key: privateKey,
            label: label || '',
            added_at: new Date().toISOString()
        };
        accounts.push(account);
        this.saveAccounts(accounts);
        console.log('[ProfileManager] Added account:', publicKey.substring(0, 20) + '...');
        return account;
    }

    removeAccount(publicKey) {
        let accounts = this.getAccounts();
        accounts = accounts.filter(a => a.public_key !== publicKey);
        this.saveAccounts(accounts);
        console.log('[ProfileManager] Removed account:', publicKey.substring(0, 20) + '...');
        return accounts;
    }

    // Set the active account. Updates the nostr_keypair localStorage entry.
    setActiveAccount(publicKey) {
        const accounts = this.getAccounts();
        const account = accounts.find(a => a.public_key === publicKey);
        if (!account) {
            console.error('[ProfileManager] Account not found:', publicKey.substring(0, 20) + '...');
            return false;
        }
        localStorage.setItem(this.ACTIVE_KEY, JSON.stringify({
            private_key: account.private_key,
            public_key: account.public_key
        }));
        return true;
    }

    updateAccountLabel(publicKey, label) {
        const accounts = this.getAccounts();
        const account = accounts.find(a => a.public_key === publicKey);
        if (account) {
            account.label = label;
            this.saveAccounts(accounts);
        }
    }

    // Returns the best display name for an account:
    // label > cached profile display_name > cached profile name > truncated npub
    getAccountDisplayName(publicKey) {
        const accounts = this.getAccounts();
        const account = accounts.find(a => a.public_key === publicKey);
        if (account && account.label) {
            return account.label;
        }

        // Check cached nostr profile
        try {
            const cached = localStorage.getItem(this.PROFILES_CACHE_KEY);
            if (cached) {
                const profile = JSON.parse(cached)[publicKey];
                if (profile?.fields?.display_name) return profile.fields.display_name;
                if (profile?.fields?.name) return profile.fields.name;
            }
        } catch (e) {
            // ignore
        }

        // Fallback to truncated npub
        if (publicKey && publicKey.length > 16) {
            return publicKey.substring(0, 12) + '...' + publicKey.substring(publicKey.length - 4);
        }
        return publicKey || 'Unknown';
    }

    // Get the avatar initial for an account
    getAccountInitial(publicKey) {
        const name = this.getAccountDisplayName(publicKey);
        return name.charAt(0).toUpperCase();
    }

    // Get the avatar picture URL from cached profile, if available
    getAccountPicture(publicKey) {
        try {
            const cached = localStorage.getItem(this.PROFILES_CACHE_KEY);
            if (cached) {
                const profile = JSON.parse(cached)[publicKey];
                return profile?.fields?.picture || null;
            }
        } catch (e) {
            // ignore
        }
        return null;
    }

    getAccountCount() {
        return this.getAccounts().length;
    }
}

window.ProfileManager = ProfileManager;
window.profileManager = new ProfileManager();
