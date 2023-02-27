

from omegaconf import OmegaConf
from psidash.psidash import load_app, load_conf, load_dash, load_components, get_callbacks, assign_callbacks
# import dash_auth # replace with flask-login
# could use flask login for added layer of security
# from flask_login import LoginManager, UserMixin
import flask
from werkzeug.middleware.proxy_fix import ProxyFix
import os
import pathlib


this_dir = pathlib.Path(__file__).parent.resolve()

conf = load_conf(f'{this_dir}/dashboard.yaml')

server = flask.Flask(__name__, # define flask app.server
    static_url_path='', # remove /static/ from url prefixes
    static_folder='static',
    )


if os.environ.get('DASH_DEBUG', 'false').lower() == 'false':
    # in production, tell flask it is behind a proxy
    # see https://flask.palletsprojects.com/en/2.2.x/deploying/proxy_fix/
    server.wsgi_app = ProxyFix(server.wsgi_app, x_for=1, x_proto=1, x_host=1, x_prefix=1)
else:
    print('Debug mode turned on')


# config
server.config.update(
    SECRET_KEY=os.urandom(12),
)


conf['app']['server'] = server

app = load_dash(__name__, conf['app'], conf.get('import'))

server = app.server

users = conf.get('users')

# auth = dash_auth.BasicAuth(app, users)

app.layout = load_components(conf['layout'], conf.get('import'))

if 'callbacks' in conf:
    callbacks = get_callbacks(app, conf['callbacks'])
    assign_callbacks(callbacks, conf['callbacks'])

if 'ssl_context' in conf['app.run_server']:
    ssl_context = conf['app.run_server']['ssl_context']
    if isinstance(ssl_context, str):
        pass
    else: # convert form list to tuple
        conf['app.run_server']['ssl_context'] = tuple(ssl_context)


if __name__ == '__main__':
    app.run_server(**conf['app.run_server'])
