# FROM continuumio/miniconda3:latest
FROM condaforge/miniforge3:latest
LABEL maintainer "Asher Pembroke <apembroke@gmail.com>"

RUN apt-get update
RUN apt-get install -y build-essential --no-install-recommends  automake pkg-config libtool libffi-dev

RUN pip3 install --no-binary :all: secp256k1

RUN conda install jupyter jupytext

COPY requirements.txt /rigly/requirements.txt

RUN pip install -r /rigly/requirements.txt

# ADD . /nostr-mail

# WORKDIR /nostr-mail

# # RUN pip install -e .

# # CMD ["nostr-mail"]

