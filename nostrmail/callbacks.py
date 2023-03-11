from utils import load_contacts, get_events
import dash_bootstrap_components as dbc
import pandas as pd
from dash.exceptions import PreventUpdate
import json

def update_contacts(url):
    contacts = load_contacts()
    return contacts

def update_contacts_options(contacts):
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
            profile = get_events(pubkey, 'meta')[0]
            return profile

def render_contact_profile(profile):
    if profile is None:
        raise PreventUpdate
    try:
        return profile['picture'], profile['display_name'], profile['about']
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

