# Typst translator
This is a small tool for translating typst documents with libretranslate. This is at the moment used to translate protocols into English.

## Usage 
Install the binary with `cargo install --path .`

You will need to have a local version of [libretranslate](https://github.com/LibreTranslate/LibreTranslate) running on your machine. This at the moment expects the libre translate api to be at `localhost:5000` 


## ToDo
- Make the follwing things user modifiable:
    - API address
    - input and output language
    - the other json payload options
- implement some of this with environment variables