/* Tom Select wrapper for the #inbox-folder-preference multiselect.
 *
 * The underlying <select multiple> remains the source of truth for the
 * settings form: TS keeps it in sync, dispatches native `change` events,
 * and our existing read paths (Array.from(el.selectedOptions)) keep
 * working. This module just owns the instance lifecycle and the
 * Gmail-aware grouping/warning behavior.
 */
(function () {
    const SELECTOR = '#inbox-folder-preference';
    const ALL_MAIL_FOLDER = '[Gmail]/All Mail';
    const ALL_MAIL_WARNING_KEY = 'nostrmail.all_mail_warning_ack';

    let instance = null;
    let lastWarningAt = 0;

    function getEl() {
        return document.querySelector(SELECTOR);
    }

    /* Split the folder list into [Gmail]/* system folders vs everything else.
       Gmail accounts can expose 100+ labels; grouping the namespace folders
       (Sent Mail, All Mail, Spam, Trash, Drafts, Important) makes the
       dropdown scannable. */
    function groupFolders(folders) {
        const gmail = [];
        const user = [];
        for (const f of folders) {
            if (f.startsWith('[Gmail]/') || f.startsWith('[Google Mail]/')) {
                gmail.push(f);
            } else {
                user.push(f);
            }
        }
        return { gmail, user };
    }

    function ensureInstance() {
        if (instance) return instance;
        const el = getEl();
        if (!el || typeof window.TomSelect === 'undefined') return null;

        instance = new window.TomSelect(el, {
            wrapperClass: 'ts-wrapper folder-multiselect',
            plugins: ['remove_button'],
            persist: false,
            create: false,
            hideSelected: true,
            closeAfterSelect: false,
            placeholder: 'Default (INBOX + nostr-mail)',
            maxOptions: 500,
            searchField: ['text', 'value'],
            onItemAdd: (value) => {
                if (value === ALL_MAIL_FOLDER) {
                    warnAllMail();
                }
            },
        });
        return instance;
    }

    function warnAllMail() {
        // Debounce so a programmatic batch-add doesn't fire multiple alerts.
        const now = Date.now();
        if (now - lastWarningAt < 1000) return;
        lastWarningAt = now;

        try {
            if (sessionStorage.getItem(ALL_MAIL_WARNING_KEY) === '1') return;
        } catch (_) { /* sessionStorage may be unavailable */ }

        const msg = 'Heads up: [Gmail]/All Mail contains every message in your account, including copies already covered by INBOX and other labels. The first sync may be slow and storage usage will be larger. Keep it selected?';
        const keep = window.confirm(msg);
        if (!keep && instance) {
            instance.removeItem(ALL_MAIL_FOLDER, true);
        } else {
            try { sessionStorage.setItem(ALL_MAIL_WARNING_KEY, '1'); } catch (_) {}
        }
    }

    /* Replace the option list. `selected` is an array of folder names to keep
       selected after the rebuild. Folders that the server no longer reports
       are dropped silently. Passing a non-empty selected list for folders
       absent from `folders` is a no-op (TS won't synthesize missing options). */
    function setOptions(folders, selected) {
        const ts = ensureInstance();
        if (!ts) return;

        const { gmail, user } = groupFolders(folders || []);

        ts.clear(true);
        ts.clearOptions();
        ts.clearOptionGroups();

        if (gmail.length > 0) {
            ts.addOptionGroup('gmail', { label: 'Gmail System Folders' });
            for (const name of gmail) {
                ts.addOption({ value: name, text: name, optgroup: 'gmail' });
            }
        }
        if (user.length > 0) {
            ts.addOptionGroup('user', { label: 'Folders' });
            for (const name of user) {
                ts.addOption({ value: name, text: name, optgroup: 'user' });
            }
        }
        ts.refreshOptions(false);

        if (Array.isArray(selected) && selected.length > 0) {
            // silent=true: skip change events + skip the All-Mail confirm
            // dialog during restore (the user already picked these previously).
            ts.setValue(selected, true);
        }
        ts.enable();
    }

    /* Seed the control with just the persisted folder names, before the live
       server list arrives. Used by populateSettingsForm so the saved value is
       visible immediately. */
    function setSelectionOnly(selected) {
        const ts = ensureInstance();
        if (!ts) return;
        ts.clear(true);
        ts.clearOptions();
        ts.clearOptionGroups();
        if (Array.isArray(selected) && selected.length > 0) {
            ts.addOptionGroup('user', { label: 'Folders' });
            for (const name of selected) {
                ts.addOption({ value: name, text: name, optgroup: 'user' });
            }
            ts.setValue(selected, true);
        }
        ts.enable();
    }

    function getSelection() {
        const el = getEl();
        if (!el) return [];
        return Array.from(el.selectedOptions).map((o) => o.value).filter(Boolean);
    }

    function setLoading() {
        const ts = ensureInstance();
        if (!ts) return;
        ts.disable();
    }

    function setReady() {
        const ts = ensureInstance();
        if (!ts) return;
        ts.enable();
    }

    /* Update the empty-state placeholder shown when nothing is selected.
       Called by the settings-load path after fetching provider-aware defaults
       from the backend, so the text reflects what will actually be synced. */
    function setPlaceholder(text) {
        const ts = ensureInstance();
        if (!ts) return;
        ts.settings.placeholder = text;
        const inputEl = ts.control_input;
        if (inputEl) inputEl.setAttribute('placeholder', text);
    }

    window.FolderMultiselect = {
        ensureInstance,
        setOptions,
        setSelectionOnly,
        getSelection,
        setLoading,
        setReady,
        setPlaceholder,
        ALL_MAIL_FOLDER,
    };
})();
