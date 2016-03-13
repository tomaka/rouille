#!/bin/sh


#Decrypt RSA key
mkdir -p ~/.ssh
openssl aes-256-cbc -K $encrypted_0a6446eb3ae3_key -iv $encrypted_0a6446eb3ae3_key -in id_rsa.enc -out ~/.ssh/id_rsa -d
chmod 600 ~/.ssh/id_rsa

cargo doc -v --no-deps

git config user.name "multipart doc upload"
git config user.email "nobody@example.com"

git branch -df gh-pages
git checkout --orphan gh-pages

git reset
git clean -d -x -f -e target/doc

cp -R target/doc .
rm -rf target

git add -A

git commit -qm "Documentation for ${TRAVIS_TAG}"
git push -f origin gh-pages
