from utils import load_contacts, get_events, load_user_profile, get_dms
import dash_bootstrap_components as dbc
from dash import html
import pandas as pd
from dash.exceptions import PreventUpdate
import json
import os
from nostr.key import PrivateKey
import dash

from redmail import EmailSender
from smtplib import SMTP

def get_triggered(ctx=None):
    if ctx is None:
        # fall back to global if callback_context is not available
        ctx = dash.callback_context
    if not ctx.triggered:
        button_id = 'No clicks yet'
    else:
        button_id = ctx.triggered[0]['prop_id'].split('.')[0]
    return button_id

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
        pubkey = contact['pubkey']
        username = f"{contact['username']} {pubkey}"
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

    #   - id: send-email
    #     attr: n_clicks
    # state:
    #   - id: user-email
    #     attr: value
    #   - id: user-password
    #     attr: value
    #   - id: receiver-address
    #     attr: value
    #   - id: subject-encrypted
    #     attr: value
    #   - id: body-encrypted
    #     attr: value

def send_mail(
        n_clicks,
        user_email,
        user_password,
        receiver_address,
        subject_encrypted,
        body_encrypted,
        smtp_host,
        smtp_port):
    """Send encrypted email"""
    if n_clicks is None:
        raise PreventUpdate
    # We only want to display the "Message sent!" when we actually send a message
    # clear the send status if the send button was not clicked
    button_id = get_triggered()

    if button_id != 'send-email':
        return button_id

    # prevent send on page load
    if n_clicks == 0:
        raise PreventUpdate

    try:

        if 'gmail' in user_email:
            from redmail import gmail

            gmail.username = user_email # Your Gmail address
            gmail.password = user_password # app password
            gmail.send(
                    subject=subject_encrypted,
                    receivers=[receiver_address],
                    text=body_encrypted,
                    )

        else:
            email = EmailSender(
                host=smtp_host,
                port=smtp_port,
                cls_smtp=SMTP,
                use_starttls=True,
                )
            email.send(
                subject=subject_encrypted,
                sender=user_email,
                receivers=[receiver_address],
                text=body_encrypted,
                )
    except Exception as m:
        return str(m)

    return f'Email sent to {receiver_address}!'


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
    credentials = dict(
        EMAIL=os.environ.get('EMAIL'),
        EMAIL_PASSWORD=os.environ.get('EMAIL_PASSWORD'),
        IMAP_HOST = os.environ.get('IMAP_HOST'),
        IMAP_PORT = os.environ.get('IMAP_PORT'),
        SMTP_HOST = os.environ.get('SMTP_HOST'),
        SMTP_PORT = os.environ.get('SMTP_PORT'),
        )
    if None in credentials.values():
        for k,v in credentials.items():
            if v is None:
                raise IOError(f'env variable {k} missing')
    print('found credentials')
    return tuple(credentials.values())


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

def decrypt_message(priv_key_nsec, pub_key_hex, encrypted_message):
    """encrypt message using shared secret"""
    if None not in (priv_key_nsec, pub_key_hex, encrypted_message):
        priv_key = PrivateKey.from_nsec(priv_key_nsec)
        return priv_key.decrypt_message(encrypted_message, pub_key_hex)
    raise PreventUpdate

def update_inbox(priv_key_nsec):
    priv_key = PrivateKey.from_nsec(priv_key_nsec)
    dms = pd.DataFrame(get_dms(priv_key.public_key.hex()))
    dms_render = []
    style = dict(
          display="inline-block",
          width="50px",
          height="50px",
          borderRadius="50%",
          backgroundRepeat="no-repeat",
          backgroundRosition="center center",
          backgroundSize="cover",)
    for author, convos in dms.groupby('author'):
        profile = load_user_profile(author)
        style_ = style.copy()
        style_.update(backgroundImage=f"url({profile['picture']})")
        convos.set_index('time', inplace=True)
        convos.sort_index(ascending=False, inplace=True)
        convos_list = []
        for _, conv in convos.iterrows():
            convos_list.append(
                dbc.ListGroup([
                        dbc.ListGroupItem(conv['content'], n_clicks=0, action=True),
                        dbc.ListGroupItem(str(_))],
                    horizontal=True)
                )

        dms_render.append(dbc.Row([
            dbc.Col(html.Div(style=style_.copy()), width=1),
            dbc.Col(convos_list),
            ]))

    return dms_render


