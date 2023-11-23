import base64
import os
from cryptography.hazmat.primitives import hashes, hmac
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.backends import default_backend

# Assuming utf8_to_bytes and bytes_to_utf8 are utility functions to handle string encoding
def utf8_to_bytes(s):
    return s.encode('utf-8')

def bytes_to_utf8(b):
    return b.decode('utf-8')

# Utility functions similar to the TypeScript version
def get_conversation_key(privkeyA, pubkeyB):
    # Implementation depends on the secp256k1 library you choose
    pass

def calc_padding(length):
    if not isinstance(length, int) or length < 0:
        raise ValueError('expected positive integer')
    if length <= 32:
        return 32
    next_power = 1 << (length - 1).bit_length()
    chunk = next_power <= 256 and 32 or next_power // 8
    return chunk * ((length - 1) // chunk + 1)

def pad(unpadded):
    unpadded_bytes = utf8_to_bytes(unpadded)
    length = len(unpadded_bytes)
    if length < 1 or length >= (65536 - 128):
        raise ValueError('invalid plaintext length')
    padded_length = calc_padding(length)
    padding = b'\x00' * (padded_length - length)
    length_bytes = length.to_bytes(2, 'big')
    return length_bytes + unpadded_bytes + padding

def unpad(padded):
    length = int.from_bytes(padded[:2], 'big')
    unpadded = padded[2:2 + length]
    if len(unpadded) != length or len(padded) != 2 + calc_padding(length):
        raise ValueError('invalid padding')
    return bytes_to_utf8(unpadded)

# Implementing the encrypt and decrypt functions
def encrypt(key, plaintext, salt=None, version=2):
    if version != 2:
        raise ValueError('unknown encryption version')
    salt = salt or os.urandom(32)
    hkdf = HKDF(algorithm=hashes.SHA256(), length=76, salt=salt, info=b'nip44-v2', backend=default_backend())
    keys = hkdf.derive(key)
    encryption_key = keys[:32]
    nonce = keys[32:44]
    auth_key = keys[44:]

    padded_plaintext = pad(plaintext)
    algorithm = algorithms.ChaCha20(encryption_key, nonce)
    cipher = Cipher(algorithm, mode=None, backend=default_backend())
    encryptor = cipher.encryptor()
    ciphertext = encryptor.update(padded_plaintext) + encryptor.finalize()

    h = hmac.HMAC(auth_key, hashes.SHA256(), backend=default_backend())
    h.update(ciphertext)
    mac = h.finalize()

    return base64.b64encode(bytes([version]) + salt + ciphertext + mac).decode('utf-8')

def decrypt(key, b64_ciphertext):
    data = base64.b64decode(b64_ciphertext)
    if data[0] != 2:
        raise ValueError('unknown encryption version')
    
    salt = data[1:33]
    ciphertext = data[33:-32]
    mac = data[-32:]
    
    hkdf = HKDF(algorithm=hashes.SHA256(), length=76, salt=salt, info=b'nip44-v2', backend=default_backend())
    keys = hkdf.derive(key)
    encryption_key = keys[:32]
    nonce = keys[32:44]
    auth_key = keys[44:]

    h = hmac.HMAC(auth_key, hashes.SHA256(), backend=default_backend())
    h.update(ciphertext)
    if not hmac.compare_digest(h.finalize(), mac):
        raise ValueError('invalid MAC')

    algorithm = algorithms.ChaCha20(encryption_key, nonce)
    cipher = Cipher(algorithm, mode=None, backend=default_backend())
    decryptor = cipher.decryptor()
    padded_plaintext = decryptor.update(ciphertext) + decryptor.finalize()

    return unpad(padded_plaintext)

if __name__ == '__main__':
    # Example usage:
    try:
        secret_key = os.urandom(32)  # This should be your actual secret key
        message = "Hello, World!"
        encrypted_msg = encrypt(secret_key, message)
        print(f"Encrypted message: {encrypted_msg}")
        decrypted_msg = decrypt(secret_key, encrypted_msg)
        print(f"Decrypted message: {decrypted_msg}")
