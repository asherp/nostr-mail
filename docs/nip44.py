# +
from secp256k1 import PrivateKey, PublicKey
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from cryptography.hazmat.primitives import hashes, hmac
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.backends import default_backend
from Crypto.Cipher import ChaCha20
import hmac as hmac_module  # Python's built-in hmac module


import base64
import os

# Converts a string to bytes using UTF-8 encoding
def utf8_to_bytes(s):
    return s.encode('utf-8')

# Converts bytes to a string using UTF-8 encoding
def bytes_to_utf8(b):
    return b.decode('utf-8')

# Calculates the amount of padding required for a given length of data
def calc_padding(length):
    # Padding calculation follows a specific scheme to align data
    if not isinstance(length, int) or length < 0:
        raise ValueError('expected positive integer')
    if length <= 32:
        return 32
    # Find the next power of two greater than the data length
    next_power = 1 << (length - 1).bit_length()
    # Determine chunk size based on the next power of two
    chunk = next_power <= 256 and 32 or next_power // 8
    # Calculate the padded length
    return chunk * ((length - 1) // chunk + 1)

# Applies padding to the data according to the calculated padding scheme
def pad(unpadded):
    # Convert string data to bytes
    unpadded_bytes = utf8_to_bytes(unpadded)
    length = len(unpadded_bytes)
    # Ensure that the data length is within allowable limits
    if length < 1 or length >= (65536 - 128):
        raise ValueError('invalid plaintext length')
    # Apply padding
    padded_length = calc_padding(length)
    padding = b'\x00' * (padded_length - length)
    # Prepend the length of the data in bytes to the padded data
    length_bytes = length.to_bytes(2, 'big')
    return length_bytes + unpadded_bytes + padding

# Removes padding from the data
def unpad(padded):
    # Extract the length of the data from the first two bytes
    length = int.from_bytes(padded[:2], 'big')
    # Extract the actual data based on the length
    unpadded = padded[2:2 + length]
    # Verify the padding is correct
    if len(unpadded) != length or len(padded) != 2 + calc_padding(length):
        raise ValueError('invalid padding')
    return bytes_to_utf8(unpadded)

# Encrypts the message according to NIP-44 using ChaCha20 and HMAC-SHA256
def encrypt(key, plaintext, salt=None, version=2):
    # NIP-44 specifies version 2 of the encryption standard
    if version != 2:
        raise ValueError('unknown encryption version')
    # Generate a random salt if not provided
    salt = salt or os.urandom(32)
    # Derive keys using HKDF with SHA-256 hash function
    hkdf = HKDF(algorithm=hashes.SHA256(), length=80, salt=salt, info=b'nip44-v2', backend=default_backend())
    keys = hkdf.derive(key)
    encryption_key = keys[:32]
    nonce = keys[32:44]  # nonce length to 12 bytes to match Go implementation
#     print(f'len of nonce: {len(nonce)}')
    auth_key = keys[48:]
    # Apply padding to plaintext
    padded_plaintext = pad(plaintext)
    # Encrypt using ChaCha20 algorithm
    # pycrytodome version
    cipher = ChaCha20.new(key=key, nonce=nonce)
    ciphertext = cipher.encrypt(padded_plaintext)
#     algorithm = algorithms.ChaCha20(encryption_key, nonce)
#     cipher = Cipher(algorithm, mode=None, backend=default_backend())
#     encryptor = cipher.encryptor()
#     ciphertext = encryptor.update(padded_plaintext) + encryptor.finalize()

    # Generate MAC using HMAC-SHA256
    h = hmac.HMAC(auth_key, hashes.SHA256(), backend=default_backend())
    h.update(ciphertext)
    mac = h.finalize()
    # Encode the encrypted message and MAC in base64
    return base64.b64encode(bytes([version]) + salt + ciphertext + mac).decode('utf-8')

# Decrypts the message according to NIP-44 using ChaCha20 and HMAC-SHA256
def decrypt(key, b64_ciphertext):
    # Decode the base64 encoded ciphertext
    data = base64.b64decode(b64_ciphertext)
    # Validate the encryption version
    if data[0] != 2:
        raise ValueError('unknown encryption version')

    # Extract the salt, ciphertext, and MAC from the data
    salt = data[1:33]
    ciphertext = data[33:-32]
    mac = data[-32:]

    # Derive keys using HKDF with SHA-256 hash function
    hkdf = HKDF(algorithm=hashes.SHA256(), length=80, salt=salt, info=b'nip44-v2', backend=default_backend())
    keys = hkdf.derive(key)
    encryption_key = keys[:32]
    nonce = keys[32:44]  # Correct nonce length to 12 bytes to match Go implementation
    auth_key = keys[48:]

    # Verify MAC using HMAC-SHA256 to ensure ciphertext integrity
    h = hmac.HMAC(auth_key, hashes.SHA256(), backend=default_backend())
    h.update(ciphertext)
    if not hmac_module.compare_digest(h.finalize(), mac):
        raise ValueError('invalid MAC')

    # Decrypt using ChaCha20 algorithm
    # pycrytodome version
    cipher = ChaCha20.new(key=key, nonce=nonce)
    padded_plaintext = cipher.decrypt(ciphertext)
#     algorithm = algorithms.ChaCha20(encryption_key, nonce)
#     cipher = Cipher(algorithm, mode=None, backend=default_backend())
#     decryptor = cipher.decryptor()
#     padded_plaintext = decryptor.update(ciphertext) + decryptor.finalize()

    # Remove padding from the decrypted plaintext
    return unpad(padded_plaintext)

if __name__ == '__main__':
    # Demonstration of encryption and decryption process following NIP-44 standards
    secret_key = os.urandom(32)  # Generate a 32-byte key for ChaCha20 cipher

    message = "Hello, World!"
    print(f"Original message: {message}")

    # Encrypt the message
    encrypted_msg = encrypt(secret_key, message)
    print(f"Encrypted message: {encrypted_msg}")

    # Decrypt the message
    decrypted_msg = decrypt(secret_key, encrypted_msg)
    print(f"Decrypted message: {decrypted_msg}")

    
# -

# # Test Vectors
#
# Steps that must be tested:
#
# - `valid_sec` - encrypt (calculate and compare shared key, calculate and compare ciphertext), decrypt (compare plaintext)
# - `valid_pub` - encrypt (calculate and compare shared key, calculate and compare ciphertext), decrypt (compare plaintext)
# - `invalid` - decrypt must throw an error
# - `invalid_conversation_key` - encrypt or get_conversation_key must throw an error
# - `padding` - tests for calc_padding utility method

import json

with open('nip44.vectors.json') as test_file:
    test_data = json.load(test_file)

test = test_data['v2']['valid_sec'][1]

test

ciphertext = encrypt(bytes.fromhex(test['shared']), test['plaintext'], salt=bytes.fromhex(test['salt']))
ciphertext

test['ciphertext']

decrypt(bytes.fromhex(test['shared']), ciphertext)

assert ciphertext == test['ciphertext']

# ### Test shared secret

test = test_data['v2']['valid_sec'][3]

test

sec1_string = test['sec1']
sec1_bytes = bytes.fromhex(sec1_string)
priv_key1 = PrivateKey(sec1_bytes)
priv_key1.private_key.hex()

sec2_string = test['sec2']
sec2_bytes = bytes.fromhex(sec2_string)
priv_key2 = PrivateKey(sec2_bytes)
priv_key2.private_key.hex()

# Try interpreting sec2 as public key hex

pk = secp256k1.PublicKey(b"\x02" + bytes.fromhex(sec2_string), True)
pk

pk.ecdh(priv_key1.private_key).hex() # does not match test['shared']

test['shared'] # this must not be the ecdsa shared secret

# Try interpreting sec1 as pub key hex -> results in invalid public key

# +
s1 = priv_key1.pubkey.ecdh(priv_key2.private_key).hex() # these two generate the same shared secret
s2 = priv_key2.pubkey.ecdh(priv_key1.private_key).hex()

assert s1 == s2
# -

s1

test['shared']

# ## pycryptodome
#
# https://pycryptodome.readthedocs.io/en/latest/src/cipher/chacha20.html
#
# * chacha20 in cryptography.hazmat.primitives.ciphers.algorithms requires a 16bit nonce 

# * go and typescript implmentations use a 12-byte nonce!
# * pycryptodome supports a 12-byte nonce

# +
import json
from base64 import b64encode
from Crypto.Cipher import ChaCha20
from Crypto.Random import get_random_bytes

plaintext = b'Attack at dawn'
key = get_random_bytes(32)
cipher = ChaCha20.new(key=key)
ciphertext = cipher.encrypt(plaintext)

nonce = b64encode(cipher.nonce).decode('utf-8')
ct = b64encode(ciphertext).decode('utf-8')
result = json.dumps({'nonce':nonce, 'ciphertext':ct})
result
# -

plaintext.decode()

'Attack at dawn'.encode()

# +
import json
from base64 import b64decode
from Crypto.Cipher import ChaCha20

# We assume that the key was somehow securely shared
try:
    b64 = json.loads(result)
    nonce = b64decode(b64['nonce'])
    ciphertext = b64decode(b64['ciphertext'])
    cipher = ChaCha20.new(key=key, nonce=nonce)
    plaintext = cipher.decrypt(ciphertext)
    print("The message was " + plaintext.decode())
except (ValueError, KeyError):
    print("Incorrect decryption")
# -


