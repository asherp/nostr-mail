from utils import load_contacts, get_events, load_user_profile
import dash_bootstrap_components as dbc
import pandas as pd
from dash.exceptions import PreventUpdate
import json
import os
from nostr.key import PrivateKey

def update_contacts(url):
    contacts = load_contacts()
    return contacts

def update_contacts_options(contacts):
    """Provide username selection where value is pubkey

    Note: there may be duplicate usernames, so we'll
    need to make sure usernames are unique among contacts
    """
    options = []
    for contact in contacts:
        username = contact['username']
        pubkey = contact['pubkey']
        options.append(dict(label=username, value=pubkey))
    return options

def update_contacts_table(contacts):
    df = pd.DataFrame(contacts).set_index('pubkey')
    table = dbc.Table.from_dataframe(df, index=True)
    return table.children

def update_contact_profile(pubkey, contacts):
    if contacts is None:
        raise PreventUpdate

    for contact in contacts:
        if contact['pubkey'] == pubkey:
            # profile = get_events(pubkey, 'meta')[0]
            return load_user_profile(pubkey)

def render_contact_profile(profile):
    if profile is None:
        raise PreventUpdate
    try:
        return profile['picture'], profile['display_name'], profile['about'], profile.get('email')
    except:
        print('problem rendering profile', profile)
        raise

def toggle_collapse(n, is_open):
    if n:
        return not is_open
    return is_open

def pass_through(*args):
    return args

def send_mail(*args):
    return 'debug info'


def get_username(profile):
    name = profile.get('display_name')
    return f"### Welcome, {name}!"


def get_nostr_priv_key(url):
    """if nostr credentials set by environment variable, use them"""
    priv_key_nsec = os.environ.get('NOSTR_PRIV_KEY')
    if priv_key_nsec is not None:
        return priv_key_nsec
    raise PreventUpdate

def get_nostr_pub_key(priv_key_nsec):
    if priv_key_nsec is None:
        raise PreventUpdate
    pub_key_hex = PrivateKey.from_nsec(priv_key_nsec).public_key.hex()
    return pub_key_hex

def get_email_credentials(url):
    """if credentials are set by environment variables, use them"""
    email_address = os.environ.get('EMAIL')
    email_password = os.environ.get('EMAIL_PASSWORD')
    email_imap = os.environ.get('EMAIL_IMAP')
    email_imap_port = os.environ.get('EMAIL_IMAP_PORT')
    credentials = (email_address, email_password, email_imap, email_imap_port)
    if None in credentials:
        raise IOError('one of credentials missing')
    print('found credentials')
    return email_address, email_password, email_imap, email_imap_port


def update_receiver_address(pub_key_hex):
    if pub_key_hex is not None:
        profile = load_user_profile(pub_key_hex)
        return profile.get('email')
    raise PreventUpdate

def encrypt_message(priv_key_nsec, pub_key_hex, message):
    """encrypt message using shared secret"""
    if None not in (priv_key_nsec, pub_key_hex, message):
        priv_key = PrivateKey.from_nsec(priv_key_nsec)
        return priv_key.encrypt_message(message, pub_key_hex)
    raise PreventUpdate




