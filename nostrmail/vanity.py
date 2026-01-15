from nostr.key import mine_vanity_key

import sys


if __name__ == '__main__':
	args = sys.argv[1:]
	prefix, suffix = None, None
	if len(args) == 1:
		prefix = args[0]
		print(prefix)
	elif len(args) == 2:
		prefix, suffix = args
		print(prefix, suffix)
	pk = mine_vanity_key(prefix=prefix, suffix=suffix)
	# pk = mine_vanity_key(name)
	print(f'priv key: {pk.bech32()}')
	print(f'pub key: {pk.public_key.bech32()}')
