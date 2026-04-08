// Profile Manager
// Manages multiple nostr accounts/profiles via backend keychain storage

class ProfileManager {
    constructor() {
        this.PROFILES_CACHE_KEY = 'nostr_mail_profiles';
    }

    async getAccounts() {
        try {
            return await TauriService.invoke('keychain_list_accounts');
        } catch (e) {
            console.error('[ProfileManager] Failed to list accounts:', e);
            return [];
        }
    }

    async getActiveAccount() {
        try {
            const pubkey = await TauriService.invoke('keychain_get_active_account');
            if (!pubkey) return null;
            const accounts = await this.getAccounts();
            return accounts.find(a => a.public_key === pubkey) || { public_key: pubkey, label: '', is_active: true };
        } catch (e) {
            console.error('[ProfileManager] Failed to get active account:', e);
            return null;
        }
    }

    // Add a new account. Returns the public key, or null if invalid.
    async addAccount(privateKey, label) {
        try {
            const publicKey = await TauriService.invoke('keychain_add_account', {
                privateKey: privateKey,
                label: label || ''
            });
            console.log('[ProfileManager] Added account:', publicKey.substring(0, 20) + '...');
            return { public_key: publicKey, label: label || '' };
        } catch (e) {
            console.error('[ProfileManager] Failed to add account:', e);
            return null;
        }
    }

    async removeAccount(publicKey) {
        try {
            await TauriService.invoke('keychain_remove_account', { publicKey });
            console.log('[ProfileManager] Removed account:', publicKey.substring(0, 20) + '...');
        } catch (e) {
            console.error('[ProfileManager] Failed to remove account:', e);
        }
    }

    // Set the active account in the backend.
    async setActiveAccount(publicKey) {
        try {
            await TauriService.invoke('keychain_set_active_account', { publicKey });
            return true;
        } catch (e) {
            console.error('[ProfileManager] Failed to set active account:', e);
            return false;
        }
    }

    async updateAccountLabel(publicKey, label) {
        try {
            await TauriService.invoke('keychain_update_label', { publicKey, label });
        } catch (e) {
            console.error('[ProfileManager] Failed to update label:', e);
        }
    }

    // Get private key from keychain for display in settings
    async getPrivateKey(publicKey) {
        try {
            return await TauriService.invoke('keychain_get_private_key', { publicKey });
        } catch (e) {
            console.error('[ProfileManager] Failed to get private key:', e);
            return null;
        }
    }

    // Returns the best display name for an account:
    // label > cached profile display_name > cached profile name > truncated npub
    getAccountDisplayName(publicKey) {
        // Check cached nostr profile
        try {
            const cached = localStorage.getItem(this.PROFILES_CACHE_KEY);
            if (cached) {
                const profiles = JSON.parse(cached);
                const profile = profiles[publicKey];
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

    async getAccountCount() {
        const accounts = await this.getAccounts();
        return accounts.length;
    }
}

window.ProfileManager = ProfileManager;
window.profileManager = new ProfileManager();
