# ## Nip44
#
# The spec is here https://github.com/paulmillr/nip44/blob/main/spec.md

# +
import math

# Constants as per NIP-44 spec
MAX_PLAINTEXT_SIZE = 65536 - 128

# Calculates the amount of padding required for a given length of data
def calc_padding(length):
    if length <= 32:
        return 32
    next_power = 1 << (length - 1).bit_length()
    chunk = max(32, next_power // 8)
    padded_length = chunk * ((length + chunk - 1) // chunk)
    return padded_length

# Applies padding to the data according to the calculated padding scheme
def pad(unpadded):
    if not isinstance(unpadded, str):
        raise TypeError('unpadded must be a string')
    unpadded_bytes = unpadded.encode('utf-8')
    length = len(unpadded_bytes)
    if length == 0 or length >= MAX_PLAINTEXT_SIZE:
        raise ValueError('Plaintext length must be between 1 and 65536 - 128 bytes')
    padded_length = calc_padding(length)
    padding = b'\x00' * (padded_length - length)
    length_bytes = length.to_bytes(2, 'big')
    return length_bytes + unpadded_bytes + padding

# Removes padding from the data
def unpad(padded):
    length = int.from_bytes(padded[:2], 'big')
    unpadded = padded[2:2 + length]
    if len(unpadded) != length or len(padded) != 2 + calc_padding(length):
        raise ValueError('invalid padding')
    return unpadded.decode('utf-8')


# -

cleartext = 'hello world'
assert unpad(pad(cleartext)) == cleartext

# +
# from secp256k1 import PrivateKey, PublicKey
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from cryptography.hazmat.primitives import hashes, hmac
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.backends import default_backend
from Crypto.Cipher import ChaCha20
from cryptography.hazmat.backends import default_backend
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives import serialization


import hmac as hmac_module  # Python's built-in hmac module

import base64
import os

# Converts a string to bytes using UTF-8 encoding
def utf8_to_bytes(s):
    return s.encode('utf-8')

# Converts bytes to a string using UTF-8 encoding
def bytes_to_utf8(b):
    return b.decode('utf-8')

# # Calculates the amount of padding required for a given length of data
# def calc_padding(length):
#     if not isinstance(length, int) or length < 0:
#         raise ValueError('expected positive integer')
#     if length <= 32:
#         return 32
#     next_power = 1 << (length - 1).bit_length()
#     chunk = 32 if next_power <= 256 else next_power // 8
#     return chunk * ((length - 1) // chunk + 1)

# # Applies padding to the data according to the calculated padding scheme
# def pad(unpadded):
#     # Convert string data to bytes
#     unpadded_bytes = utf8_to_bytes(unpadded)
#     length = len(unpadded_bytes)
#     # Ensure that the data length is within allowable limits
#     if length < 1 or length >= (65536 - 128):
#         raise ValueError('invalid plaintext length')
#     # Apply padding
#     padded_length = calc_padding(length)
#     padding = b'\x00' * (padded_length - length)
#     # Prepend the length of the data in bytes to the padded data
#     length_bytes = length.to_bytes(2, 'big')
#     return length_bytes + unpadded_bytes + padding

# # Removes padding from the data
# def unpad(padded):
#     # Extract the length of the data from the first two bytes
#     length = int.from_bytes(padded[:2], 'big')
#     # Extract the actual data based on the length
#     unpadded = padded[2:2 + length]
#     # Verify the padding is correct
#     if len(unpadded) != length or len(padded) != 2 + calc_padding(length):
#         raise ValueError('invalid padding')
#     return bytes_to_utf8(unpadded)

# Encrypts the message according to NIP-44 using ChaCha20 and HMAC-SHA256
def encrypt(key, plaintext, salt=None, version=2):
    # NIP-44 specifies version 2 of the encryption standard
    if version != 2:
        raise ValueError('unknown encryption version')
    # Generate a random salt if not provided
    salt = salt or os.urandom(32)
    # Derive keys using HKDF with SHA-256 hash function
    hkdf = HKDF(algorithm=hashes.SHA256(), length=76, salt=salt, info=b'nip44-v2', backend=default_backend())
    keys = hkdf.derive(key)
    encryption_key = keys[:32]
    nonce = keys[32:44]  # nonce length to 12 bytes to match Go implementation
    auth_key = keys[44:]
    # Apply padding to plaintext
    padded_plaintext = pad(plaintext)
    # Encrypt using ChaCha20 algorithm
    # pycrytodome version
    cipher = ChaCha20.new(key=key, nonce=nonce)
    ciphertext = cipher.encrypt(padded_plaintext)

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
    hkdf = HKDF(algorithm=hashes.SHA256(), length=76, salt=salt, info=b'nip44-v2', backend=default_backend())
    keys = hkdf.derive(key)
    encryption_key = keys[:32]
    nonce = keys[32:44]  # Correct nonce length to 12 bytes to match Go implementation
    auth_key = keys[44:]

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


class PrivateKey(object):
    def __init__(self, priv_key = None):
        """Initialize private key
        
        priv_key - None (default) generate new random key
                 - hex str - generate key from hex string bytes
                 - bytes - generate key from bytes
        """
        self.priv_key = None
        if priv_key is None:
            self.priv_key = ec.generate_private_key(ec.SECP256K1(), default_backend())
        elif isinstance(priv_key, str):
            # Convert the hex string to bytes
            private_value = bytes.fromhex(priv_key)
            # Convert bytes to an integer
            private_int = int.from_bytes(private_value, byteorder="big")
            # Create a private key object from the integer
            self.priv_key = ec.derive_private_key(private_int, ec.SECP256K1(), default_backend())
        elif isinstance(priv_key, bytes):
            private_int = int.from_bytes(priv_key, byteorder="big")
            # Create a private key object from the integer
            self.priv_key = ec.derive_private_key(private_int, ec.SECP256K1(), default_backend())
        elif isinstance(priv_key, int):
            # Create a private key object from the integer
            self.priv_key = ec.derive_private_key(priv_key, ec.SECP256K1(), default_backend())
        else:
            raise IOError(f'{type(priv_key)} Not yet implemented')
        self.pubkey = self.priv_key.public_key()
    
    def ecdh(self, other_pubkey):
        return self.priv_key.exchange(ec.ECDH(), other_pubkey)


# from cryptography.hazmat.backends.openssl.ec import _EllipticCurvePublicKey # do this later
# class PublicKey(object):
#     def __init__(self):
     


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


# +
import os
import hmac
import hashlib
import base64
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.backends import default_backend
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305


def encrypt(conversation_key, plaintext, salt=None, version=2):
    if version != 2:
        raise ValueError('unknown encryption version')
    if salt is None:
        salt = get_random_bytes(32)
    if len(salt) != 32:
        raise ValueError('salt must be 32 bytes')

    keys = HKDF(conversation_key, 64, salt, SHA256, 1, b'nip44-v2')
    encryption_key, nonce, auth_key = keys[:32], keys[32:44], keys[44:]
    cipher = ChaCha20.new(key=encryption_key, nonce=nonce)
    padded_plaintext = pad(plaintext)
    ciphertext = cipher.encrypt(padded_plaintext)
    hmac_ = HMAC.new(keys[32:], ciphertext, SHA256).digest()

    concat = bytes([version]) + salt + ciphertext + hmac_
    return base64.b64encode(concat).decode('utf-8')

def decrypt(conversation_key, b64_ciphertext):
    decoded = base64.b64decode(b64_ciphertext)
    version = decoded[0]
    if version != 2:
        raise ValueError('unknown encryption version')

    salt, ciphertext, hmac_ = decoded[1:33], decoded[33:-32], decoded[-32:]
    keys = HKDF(conversation_key, 64, salt, SHA256, 1, b'nip44-v2')
    encryption_key = keys[:32]
    nonce = keys[32:]
    cipher = ChaCha20.new(key=encryption_key, nonce=nonce)
    hmac_verify = HMAC.new(keys[32:], ciphertext, SHA256)
    hmac_verify.verify(hmac_)

    padded_plaintext = cipher.decrypt(ciphertext)
    return unpad(padded_plaintext)

# Example usage
try:
    secret_key = get_random_bytes(32)  # This would be the conversation key in a real scenario
    message = "Hello, World!"

    # Encrypt the message
    encrypted_msg = encrypt(secret_key, message)
    print(f"Encrypted message: {encrypted_msg}")

    # Decrypt the message
    decrypted_msg = decrypt(secret_key, encrypted_msg)
    print(f"Decrypted message: {decrypted_msg}")

    assert message == decrypted_msg, "Decrypted message does not match the original"
except Exception as e:
    print(f"An error occurred: {e}")
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

# ### Padding

# Example usage
try:
    test_string = "test"
    padded_data = pad(test_string)
    print("Padded data:", padded_data.hex())
    assert unpad(padded_data) == test_string, "Unpad did not correctly reverse pad"
except Exception as e:
    print("Error:", e)


# +
# Function to run the padding tests
def run_padding_tests(test_cases):
    all_tests_pass = True
    for unpadded_len, expected_padded_len in test_cases:
        calculated_padded_len = calc_padding(unpadded_len)
        if calculated_padded_len != expected_padded_len:
            print(f"Test failed for unpadded length {unpadded_len}: expected {expected_padded_len}, got {calculated_padded_len}")
            all_tests_pass = False
    return all_tests_pass

padding_test_cases = test_data['v2']['padding']
# Run the tests
run_padding_tests(padding_test_cases)
# -

# ### valid_sec

# +
from coincurve import PrivateKey

valid_sec_test_vectors[0]['sec1']

test = valid_sec_test_vectors[0]
priv_key1 = PrivateKey(bytes.fromhex(test['sec1']))
priv_key2 = PrivateKey(bytes.fromhex(test['sec2']))

sk1 = priv_key1.public_key.multiply(priv_key2.secret)

sk2 = priv_key2.public_key.multiply(priv_key1.secret)

assert sk1 == sk2

# +
from pynostr.key import PrivateKey, PublicKey

# Test vectors for the 'valid_sec' section
valid_sec_test_vectors = test_data['v2']['valid_sec']

# Assuming valid_sec_test_vectors is a list of dictionaries that contain sec1, sec2, and shared key.
def validate_valid_sec(test_vectors):
    for test in test_vectors:
        # Initialize private keys from the test vectors
        priv_key1 = PrivateKey.from_hex(test['sec1'])
        priv_key2 = PrivateKey.from_hex(test['sec2'])
        
        # Compute ECDH shared secrets priv_key1.ecdh(test['pub2']).hex()
        sk1 = priv_key1.ecdh(priv_key2.public_key.hex()).hex()
        sk2 = priv_key2.ecdh(priv_key1.public_key.hex()).hex()
        # Verify that the shared secrets are equal
        assert sk1 == sk2, "ECDH shared secrets do not match"
        
        # Verify that the shared secrets match the expected shared key
        expected_shared = test['shared']
        try:
            assert sk1 == expected_shared, "Computed shared secret does not match the expected value"
        except:
            print(sk1)
            print(expected_shared)
            raise


    print("All valid_sec test vectors passed.")
    return True

# Run validation
validate_valid_sec(valid_sec_test_vectors)

# -

# ## valid_pub

test = valid_pub_test_vectors[0]


priv_key1 = PrivateKey.from_hex(test['sec1'])

pub_key2 = PublicKey.from_hex(test['pub2'])

pub_key2

shared_secret = priv_key1.ecdh(pub_key2.hex())
shared_secret.hex()

test['shared']

test['plaintext']

print(test['ciphertext'])
print(len(test['ciphertext']))

# +
# encrypt?
# -

test['salt']

encrypted_msg = encrypt(shared_secret, test['plaintext'], bytes.fromhex(test['salt']))
print(encrypted_msg)
print(len(encrypted_msg))

assert decrypt(shared_secret, encrypted_msg) == test['plaintext']


# We are are able to encrypt/decrypt using the correct shared secret, but ooks like encryption is slightly off.

# +
# Test function for 'valid_pub' section
def validate_valid_pub(test_vectors):
    for test in test_vectors:
        # Generate the private key from the provided hex string
        priv_key1 = PrivateKey(bytes.fromhex(test['sec1']))
        # Convert recipient's public key hex to an actual PublicKey object
        pub_key2 = PublicKey.from_secret(bytes.fromhex(test['pub2']))

        # Perform the ECDH operation and get the shared secret
        shared_secret = pub_key2.multiply(priv_key1.secret).format()[1:]

        # Encrypt the message using the sender's shared secret
        encrypted_message = encrypt(shared_secret, test['plaintext'])
        assert decrypt(encrypted_message, shared_secret) == test['plaintext']

        # Base64 decode the expected ciphertext for comparison
        expected_ciphertext = test['ciphertext']

        # Compare the result with the expected ciphertext
        if encrypted_message != expected_ciphertext:
            print(f"Test failed for sec1 {test['sec1']}:")
            print(f"expected {expected_ciphertext}")
            print(f"got {encrypted_message}")
            return False
        else:
            print(f"Test passed for sec1 {test['sec1']}.")

    print("All 'valid_pub' test vectors passed.")
    return True

valid_pub_test_vectors = test_data['v2']['valid_pub']
# -

test = valid_pub_test_vectors[0]
test

from pynostr.key import PrivateKey, PublicKey

priv_key1.ecdh(test['pub2']).hex()

priv_key1.public_key.hex()

test['shared']

# +
priv_key1 = PrivateKey.from_hex(test['sec1'])
# Convert recipient's public key hex to an actual PublicKey object
pub_key2 = PublicKey.from_hex(test['pub2'])

# Perform the ECDH operation and get the shared secret
shared_secret = pub_key2.multiply(priv_key1.secret).format()[1:]
shared_secret
# -

shared_secret.hex()

test['shared']

validate_valid_pub(valid_pub_test_vectors)

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


