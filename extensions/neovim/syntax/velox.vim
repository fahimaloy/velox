" Vim syntax file for Velox .vx files
" Language: Velox (Rust-based reactive UI framework)
" Maintainer: Velox Contributors

if exists("b:current_syntax")
  finish
endif

" Velox is a Vue-like SFC format with Rust in script blocks
" We embed HTML, Rust, and CSS syntax

if !exists("main_syntax")
  let main_syntax = 'velox'
endif

" Template section: embed HTML with Vue-like directives
syn region veloxTemplate matchgroup=veloxTag start="<template\>" end="</template>"me=e-11 contains=veloxTemplateTag,veloxDirective,veloxInterpolation,htmlComment
syn region veloxTemplateTag matchgroup=veloxTag start="<\z([^/ >]\+\)" end="[/]*>" contains=veloxAttr,veloxDirectiveAttr,veloxEventAttr,veloxPropAttr
syn region veloxTemplateEndTag matchgroup=veloxTag start="</" end=">" contains=veloxTag

" Vue-like directives
syn match veloxDirectiveAttr "v-\a\+\>" contained
syn match veloxDirectiveAttr "@\a\+\>" contained
syn match veloxDirectiveAttr ":\a\+\>" contained

" Interpolation {{ expr }}
syn region veloxInterpolation matchgroup=veloxInterpolationDelimiter start="{{" end="}}" contained

" Script section: embed Rust
syn region veloxScript matchgroup=veloxTag start="<script\(\_s\+setup\)\?\>" end="</script>"me=e-9 contains=@rustTop

" Style section: embed CSS
syn region veloxStyle matchgroup=veloxTag start="<style\(\_s\+scoped\)\?\>" end="</style>"me=e-8 contains=@css

" HTML comments
syn region htmlComment start="<!--" end="-->" contained

" Define the default highlighting
hi def link veloxTag Tag
hi def link veloxDirectiveAttr Special
hi def link veloxEventAttr Special
hi def link veloxPropAttr Special
hi def link veloxInterpolationDelimiter Delimiter

let b:current_syntax = "velox"
