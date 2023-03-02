
def toggle_collapse(n, is_open):
    if n:
        return not is_open
    return is_open

def pass_through(*args):
    return args

def send_mail(*args):
    return 'debug info'