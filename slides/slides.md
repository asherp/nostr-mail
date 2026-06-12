---
theme: seriph
title: Nostr-Mail
info: |
  End-to-end encrypted email over the Nostr protocol.
class: text-center
highlighter: shiki
drawings:
  persist: false
transition: slide-left
mdc: true
controls: false       # hide bottom toolbar
hideInToc: true
colorSchema: dark
---
        

<style>                                                                                                                                                                       
#slidev-goto-dialog { display: none !important; }                                                                                                                             
</style>       

# Nostr-Mail

## Learning from three decades of PGP

### Freedom Tech Summit

2026-06-10 | Prague, CZE



<div class="abs-br m-6 text-xl opacity-50">
https://x.com/asherp
</div>

---

<img src="./btcpp-nostr-mail-2023.png" class="h-[32rem] mx-auto" />




---

<img src="./nostr-mail-website-2.png" class="h-[28rem] mx-auto" />


---

<img src="./zapstore_release_1.0.8.png" class="h-[28rem] mx-auto" />


<!-- 
# Outline


<v-clicks>

- Raison d'etre
- PGP: What went wrong
- Glossia
- Nostr-mail
- Demo!
- The future..

</v-clicks>
 -->
<!--
Frame: this is why we built nostr-mail.
-->

---

# Raison d'etre (reason for being)

<div class="grid grid-cols-[2fr_3fr] gap-6 items-start">

<div>

Signatures drive society...


Nostr-mail aims for mass adoption

* Digital signatures take center stage
* Full E2E encryption accross email servers
* Should we seek to be invisible or should users be aware?

We need to solve the UX problems first...

</div>

<v-click>

<img src="./dont-try.png" class="block max-h-[90vh] w-auto mx-auto object-contain my-0" />

</v-click>

</div>


---

# Why didn't PGP achieve mass adoption?

|   | PGP | Email |
|------|------|------|
| Adoption Era | 1991 (35+ years old) | Mid-1990s (hotmail, yahoo, aol..) |
| Count | ~4M keys | ~5B accounts |
| Active users | ~1–3M? | Billions |
| User base | Power users | Businesses, individuals, spammers.. |



---


# PGP: What went wrong


1) Wire format appears toxic to normies.

<br>
<br>

```mermaid {scale: 0.8}
flowchart LR
    subgraph S1["Plaintext"]
        A["The quick brown fox jumps over the lazy dog"]
    end

    subgraph S2["PGP Encryption"]
        B["Hybrid scheme:<br/>• Generate session key<br/>• AES encrypt<br/>• Encrypt key (RSA/ECC)"]
    end

    subgraph S3["Wire format"]
        C["-----BEGIN PGP MESSAGE-----<br/>hQEMA4Xk9vJ7Q+8DAQg...<br/>-----END PGP MESSAGE-----"]
    end

    A --> B --> C
```
---


# PGP: What went wrong

2) Broken workflows

Email is hostile to existing ciphertext formats...

<v-clicks>

- Headers can get stripped... needs inline approach
- Reply and forwards `> space tab \r \n < / "` breaks base64
- Reply/forward decrypts *first*
- Can't encrypt subject lines
- Signatures can't be forwarded
- Docusign/Google enter the chat...

</v-clicks>

---

# PGP: What went wrong

3. Identity/Key management

- Identity lives with the key..
- No native key discovery
- Multi-device is painful
- Migration paths?
- Web of trust

---



# Are we making the same mistakes?


<img src="./nostr-mail-slides-motivation.png" class="h-86 mx-auto my-4" />


<v-click>

<div class="flex justify-center">

"Hey, I think your app is broken!"

</div>

</v-click>
---

# Making ciphertext human friendy

Goal: go from ciphertext -> plaintext

we want:

* User-friendly
* Authoritative
* Machine-readable
* Passes spam filters? Spam rescue?

<v-click>

Why not use bip39?

`YcpOMlHxKmkKYffm4nE4Iqfv9U3wCUUefrvMn5rSbCY=`

`absurd gift fame milk physical bargain cruel civil discover tragic bean evoke earn leave wide daughter abuse eagle view puzzle odor whale hard rack scale`

</v-click>

---

# Making ciphertext human friendy

BIP-39: a great start

https://github.com/bitcoin/bips/tree/master/bip-0039

1. Words are 4-8 letters long.
2. Words can be uniquely determined typing the first 4 letters.
3. No words with accents or hyphens.
4. No words with Levehnstein distance less than 2 (Minimum Levehnstein distance for the full set of words is 2).
5. Words are sorted according to English alphabet
6. No words containing "j" or "ly" (as they are pronounced the same way, written differently)
7. No words already used in other language mnemonic sets.
8. Obscenities and bad language removed as much as possible

---


# Making ciphertext human friendy


<div class="relative min-h-64">

<div class="absolute inset-0 flex justify-center">

<img src="./nostr-mail-slides-bip39.png" class="h-70 my-4" />

</div>

<div v-click="[1, 2]" class="absolute inset-0 flex justify-center">

<img src="./nostr-mail-slides-bip39-german.png" class="h-70 my-4" />

</div>

<div v-click="2" class="absolute inset-0 flex justify-center">

<img src="./nostr-mail-slides-bip39-langs.png" class="h-70 my-4" />

</div>

</div>

---


# Why the new wordlists were rejected

The authors of these wordlists painstakingly followed all the rules in their PRs

All were rejected, because the problem turned out to be beyond the scope of bitcoin itself

This is a language-UX-culture problem

We need a project that can serve to represent binary data in human language


---

# Glossia: A universal linguistic codec

https://glossia.io/

```mermaid {scale: 0.8}
%%{init: {'flowchart': { 'htmlLabels': true, 'wrap': false}}}%%                                                                                          

flowchart LR                                                                                                                                                                  
  A["acDDcibzuQa8zOzX85OGvkKmUgqrAsWOdQJAXdRSIWo="] --> B["absurd hawk alcohol symptom evil describe local veteran outside subject orient ticket lady clerk cinnamon click
gate sheriff inhale dog level tail cinnamon mad divorce"]                                                                                                                     
  B --> C["<u>Absurd</u> may <u>hawk</u> <u>alcohol</u> to <u>symptom</u>. <u>Evil</u> <u>describe</u> the <u>local</u> <u>veteran</u>. Its <u>outside</u> <u>subject</u>
<u>orient</u>. <u>Ticket</u> see <u>lady</u> to <u>clerk</u>. <u>Cinnamon</u> <u>click</u> <u>gate</u> via <u>sheriff</u>. Cut <u>inhale</u> <u>dog</u> to <u>level</u>.      
<u>Tail</u> get a <u>cinnamon</u> via a <u>mad</u> <u>divorce</u>."]
```

---

<!-- # Glossia: A universal linguistic codec -->

<div class="w-full h-full overflow-hidden flex items-center justify-center">
  <iframe
    src="https://glossia.io/editor.html"
    class="border-0 rounded"
    style="width: 1380px; height: 760px; transform: scale(0.71); transform-origin: center;"
  ></iframe>
</div>

---


# Glossia: A universal linguistic codec

Sentence constructor (madlib generator)

- Built on Montague Grammar — formal semantics using typed lambda calculus 
- assign meanings compositionally: semantic type (N: e → t, V: e → (e →
   t), Adj: e → e)
- Languages use disjoint wordlists
- Dialects share wordlists, but differ on grammar/cover words
- General enough to support other modalities (images, music, mathematics, software languages)

* **payload** words carry data
* **cover words** get stripped
* Wordlists can be any power of 2 (Latin has 2^15)
* Includes bip39 English
* Other languages forthcoming - PRs welcome!

---


<div class="relative h-[28rem]">

<img v-click="[0, 1]"   src="./demo-01.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[1, 2]"   src="./demo-02.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[2, 3]"   src="./demo-03.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[3, 4]"   src="./demo-04.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[4, 5]"   src="./demo-05.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[5, 6]"   src="./demo-06.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[6, 7]"   src="./demo-07.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[7, 8]"   src="./demo-08.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[8, 9]"   src="./demo-09.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[9, 10]"  src="./demo-10.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[10, 11]" src="./demo-11.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[11, 12]" src="./demo-12.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[12, 13]" src="./demo-13.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[13, 14]" src="./demo-14.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[14, 15]" src="./demo-15.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[15, 16]" src="./demo-16.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[16, 17]" src="./demo-17.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[17, 18]" src="./demo-18.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[18, 19]" src="./demo-19.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="[19, 20]" src="./demo-20.png" class="absolute inset-0 m-auto max-h-full max-w-full" />
<img v-click="20" src="./demo-messages.png" class="absolute inset-0 m-auto max-h-full max-w-full" />

</div>

<!--
- Compose → encrypt → send to a test npub
- Show received message decoded back to plaintext
- Toggle dialects and re-encode the same ciphertext
-->

---

# Gmail view

<img src="./gmail-ai-view.png" class="max-h-[70vh] max-w-full mx-auto my-4" />

---

# Other projects in this space

|   | **NostrMail** (nostr-mail.com) | NMail (NostrMail.org) | ProtonMail |
|---|---|---|---|
| Transport | SMTP + Kind 14 | Relay + Kind 1301 | SMTP |
| E2E across Gmail/Yahoo/etc | ✅ | ❌ | ❌  |
| Subject encryption | ✅ | ❌ | ❌ |
| Server trust | Trustless | Trusted gateway | Trusted server |
| IMAP | ✅ | ❌ | Paid |
| Inline sig/ciphertext ("chain mail") | ✅ | ❌ | ❌ |
| Crypto | NIP-04 / NIP-44 | NIP-44 /NIP-59 | PGP |



---

# The future?


<div class="grid grid-cols-2 gap-6 ">

<v-clicks>


* Can run Cashu mint behind an email server...

* CC anonymity sets?

* Frost signatures [frostr](https://frostr.org/)

* Onion routing/mixnet

* Glossia extensions: music, images, ... accessiblity?

* kind 1301 - compat with [NMail](https://nostrmail.org/#protocol)

</v-clicks>

<v-click>

<img src="./glossia-image.png" class="max-h-[60vh] w-full object-contain" />

</v-click>

</div>


---
layout: center
class: text-center
---

# Thank you!

<img src="./qrcode_nostrmail.png" class="max-h-[40vh] mx-auto object-contain" />

