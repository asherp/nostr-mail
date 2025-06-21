## Profiles

Nostr-mail profiles require the `email` keyword to be available.

Profiles may be loaded with the function `load_user_profile`.

To test this, we'll assume Alice's priv key is stored in an environment variable.

```python
from nostrmail.callbacks import load_user_profile, get_nostr_pub_key
import os
```

```python
alice_pub_key_hex = get_nostr_pub_key(os.environ['PRIV_KEY_ALICE'])
alice_pub_key_hex
```

```python
profile = load_user_profile(alice_pub_key_hex)
```

```python
profile
```

Run the following cell to autoreload modifications to any above imports.

```python
%load_ext autoreload
%autoreload 2
```
