from nostrmail.utils import load_contacts, get_events, get_dms, get_convs, cache
from nostrmail.utils import publish_direct_message, email_is_logged_in, find_email_by_subject, get_encryption_iv
from nostrmail.utils import publish_profile
import dash_bootstrap_components as dbc

from dash import html, dcc
import pandas as pd
from dash.exceptions import PreventUpdate
import json
import os
from nostr.key import PrivateKey
import dash

import imaplib
from redmail import EmailSender
from smtplib import SMTP



def refresh_cache(n_clicks):
    if n_clicks > 0:
        cache.clear()
        return f"cache cleared {pd.Timestamp.utcnow().strftime('%Y-%m-%d %X')}"
    else:
        raise PreventUpdate


def get_triggered(ctx=None):
    if ctx is None:
        # fall back to global if callback_context is not available
        ctx = dash.callback_context
    if not ctx.triggered:
        button_id = 'No clicks yet'
    else:
        button_id = ctx.triggered[0]['prop_id'].split('.')[0]
    return button_id

@cache.memoize(tag='profiles') #Todo add temporal caching/refresh button
def load_user_profile(pub_key_hex):
    print(f'fetching profile {pub_key_hex}')
    profile_events = get_events(pub_key_hex, 'meta')
    if len(profile_events) > 0:
        profile = profile_events[0]
        return profile


def update_contacts(refresh_clicks, contacts):
    if refresh_clicks is None:
        # prevent the None callbacks is important with the store component.
        # you don't want to update the store for nothing.
        raise PreventUpdate

    if contacts is None:
        contacts = load_contacts()
    return contacts

def update_contacts_options(ts, contacts):
    """Provide username selection where value is pubkey

    Note: there may be duplicate usernames, so we'll
    need to make sure usernames are unique among contacts
    """
    if None in (ts, contacts):
        raise PreventUpdate  
    options = []
    for contact in contacts:
        pubkey = contact['pubkey']
        username = f"{contact['username']} {pubkey}"
        options.append(dict(label=username, value=pubkey))
    return options

def update_contacts_table(ts, contacts):
    if None in (ts, contacts):
        raise PreventUpdate  
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

def render_profile(profile):
    if profile is None:
        raise PreventUpdate
    try:
        return (profile.get('picture', ''),
            profile.get('display_name', 'N/A'),
            profile.get('about', 'N/A'),
            profile.get('email', 'N/A'))
    except:
        print('problem rendering profile', profile)
        raise

def toggle_collapse(n, is_open):
    if n:
        return not is_open
    return is_open

def pass_through(*args):
    return args

def send_mail(
        n_clicks,
        user_email,
        user_password,
        user_priv_key,
        receiver_pub_key,
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
        # publish the dm to nostr 
        priv_key = PrivateKey.from_nsec(user_priv_key)
        publish_direct_message(priv_key, receiver_pub_key, dm_encrypted=subject_encrypted)

        # use the same dm as the email subject
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
    if profile is None:
        raise PreventUpdate
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
        EMAIL_ADDRESS=os.environ.get('EMAIL_ADDRESS'),
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


    # input:
    #   - id: nostr-priv-key
    #     attr: value
    #   - id: decrypt-inbox
    #     attr: value
    #   - id: user-email
    #     attr: value
    #   - id: user-password
    #     attr: value
    #   - id: imap-host
    #     attr: value
    #   - id: imap-port
    #     attr: value
    #   - id: refresh-button
    #     attr: children

def update_inbox(
        active_tab,
        priv_key_nsec,
        decrypt,
        user_email,
        user_password,
        imap_host,
        imap_port):
    if active_tab != 'inbox':
        raise PreventUpdate
    # Set up connection to IMAP server
    try:
        mail = imaplib.IMAP4_SSL(host=imap_host)
    except:
        return html.Div(children=f'Cannot connect to imap host: {imap_host}')
    # if not email_is_logged_in(mail):
    print('logging in')
    mail.login(user_email, user_password)
    mail.select('Inbox')

    priv_key = PrivateKey.from_nsec(priv_key_nsec)
    pub_key = priv_key.public_key.hex()
    dms = pd.DataFrame(get_dms(pub_key))
    dms['conv'] = get_convs(dms)

    dms_render = []
    style = dict(
          display="inline-block",
          width="50px",
          height="50px",
          borderRadius="50%",
          backgroundRepeat="no-repeat",
          backgroundRosition="center center",
          backgroundSize="cover")

    for conv_id, conv in dms.groupby('conv'):
        # print(f'conv id: {conv_id}')
        conv.set_index('time', inplace=True)
        conv.sort_index(ascending=True, inplace=True)
        msg_list = []
        for _, msg in conv.iterrows():
            # print(f' msg id: {_}')
            profile = load_user_profile(msg.author)
            style_ = style.copy()
            try:
                style_.update(backgroundImage=f"url({profile['picture']})")
            except:
                raise IOError(f'could not extract picture from {profile} author: {msg.author}')
            content = msg['content']
            msg_iv = get_encryption_iv(content)
            email_body = find_email_by_subject(mail, msg_iv)

            if decrypt:
                if msg.author == pub_key: # sent from the user
                    content = priv_key.decrypt_message(content, msg['p'])
                    if email_body is not None:
                        email_body = priv_key.decrypt_message(email_body, msg['p'])
                else: # sent to the user
                    content = priv_key.decrypt_message(content, msg.author)
                    if email_body is not None:
                        email_body = priv_key.decrypt_message(email_body, msg.author)
            if email_body is not None:
                content = html.Details([
                    html.Summary(content),
                    html.Hr(),
                    dcc.Markdown(email_body.replace('\n', '<br>'),
                        dangerously_allow_html=True),])

            if msg.author == pub_key: # sent from the user
                msg_list.append(
                    dbc.ListGroup([
                            dbc.ListGroupItem(html.Div(style=style.copy())),
                            dbc.ListGroupItem(content, n_clicks=0, action=True),
                            dbc.ListGroupItem(str(_)),
                            dbc.ListGroupItem(html.Div(style=style_.copy())),],
                        horizontal=True)
                    )
            else: # sent to the user
                msg_list.append(
                    dbc.ListGroup([
                            dbc.ListGroupItem(html.Div(style=style_.copy())),
                            dbc.ListGroupItem(content, n_clicks=0, action=True),
                            dbc.ListGroupItem(str(_)),
                            dbc.ListGroupItem(html.Div(style=style.copy())),
                            ],
                        horizontal=True)
                    )   

        # print('appending messages')
        dms_render.append(dbc.Row(dbc.Col(msg_list)))

    # Close the mailbox and logout from the IMAP server
    if email_is_logged_in(mail):
        print('logging out')
        try:
            mail.close()
            mail.logout()
        except:
            pass

    return dms_render


def edit_user_profile(profile):
    """render the current profile to editable fields"""
    if profile is None:
        profile = {}

    children = []
    for _ in ['display_name', 'name', 'picture', 'about', 'email']:
        if _ not in profile:
            profile[_] = None

    for k, v in profile.items():
        children.append(dbc.Row(
            children=[
                dbc.FormFloating(
                    children=[
                        dbc.Input(
                            # allow pattern matching to extract these later
                            id=dict(form_type='user_profile_values', form_key=k),
                            value=v),
                        dbc.Label(
                            id=dict(form_type='user_profile_keys', form_key=k),
                            children=k),
                        html.Br(),
                    ])
            ]))
    return children

def update_user_profile(n_clicks, priv_key_nsec, profile_keys, profile_values):
    if n_clicks == 0:
        raise PreventUpdate


    profile = {}
    for k, v in zip(profile_keys, profile_values):
        profile[k] = v

    priv_key = PrivateKey.from_nsec(priv_key_nsec)
    sig = publish_profile(priv_key, profile)

    return sig

