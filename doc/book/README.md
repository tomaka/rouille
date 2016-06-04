# Introduction

Welcome the *rouille* library!

*Rouille* is a micro-web-framework that is built around the following concept:
don't do anything magical.

With the exception of the handling of the HTTP protocol, you know *exactly* what is happening
at any time.
Each instruction is executed one after another. There is no callback, no middleware, no filter
that intercepts and changes your data without telling you. If your website doesn't give the
correct result, it is trivial to find out why.
