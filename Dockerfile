# FROM continuumio/miniconda3:latest
FROM condaforge/miniforge3:latest
LABEL maintainer "Asher Pembroke <apembroke@gmail.com>"

RUN apt-get update
RUN apt-get install -y build-essential --no-install-recommends  automake pkg-config libtool libffi-dev

RUN pip3 install --no-binary :all: secp256k1

RUN conda install jupyter jupytext

COPY requirements.txt /nostrmail/requirements.txt

RUN pip install -r /nostrmail/requirements.txt
# RUN pip install git+https://github.com/jeffthibault/python-nostr.git@37cb66ba2d3968b2d75cc8ad71c3550415ca47fe

ENV PYTHONUNBUFFERED 1

ADD . /nostrmail

WORKDIR /nostrmail

RUN pip install -e .

CMD ["python", "nostrmail/dashboard.py"]

