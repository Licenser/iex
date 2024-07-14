use quote::quote;
use syn::{
    parse_macro_input, parse_quote, parse_quote_spanned,
    spanned::Spanned,
    visit_mut::{visit_expr_mut, VisitMut},
    Expr, ExprClosure, ExprTry, Generics, ItemFn, ReturnType, Signature, Type,
};

struct ReplaceTry;
impl VisitMut for ReplaceTry {
    fn visit_expr_mut(&mut self, node: &mut Expr) {
        if let Expr::Try(ExprTry { expr, .. }) = node {
            *node = parse_quote!(::iex::Outcome::get_value_or_panic(#expr, _unsafe_iex_marker));
        }
        visit_expr_mut(self, node);
    }
    fn visit_item_fn_mut(&mut self, _node: &mut ItemFn) {
        // Don't recurse into other functions or closures
    }
    fn visit_expr_closure_mut(&mut self, _node: &mut ExprClosure) {
        // Don't recurse into other functions or closures
    }
}

#[proc_macro_attribute]
pub fn iex(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as ItemFn);
    let input_span = input.span();

    let result_type = match input.sig.output {
        ReturnType::Default => parse_quote! { () },
        ReturnType::Type(_, ref result_type) => result_type.clone(),
    };
    let output_type: Type = parse_quote! { <#result_type as ::iex::Outcome>::Output };
    let error_type: Type = parse_quote! { <#result_type as ::iex::Outcome>::Error };
    let to_impl_outcome: ReturnType = parse_quote! {
        -> impl ::iex::Outcome<
            Output = #output_type,
            Error = #error_type,
        >
    };

    let mut where_clause = input
        .sig
        .generics
        .where_clause
        .clone()
        .unwrap_or(parse_quote! { where });
    where_clause
        .predicates
        .push(parse_quote_spanned! { result_type.span() => #result_type: ::iex::Outcome });
    let wrapper_sig = Signature {
        generics: Generics {
            where_clause: Some(where_clause),
            ..input.sig.generics.clone()
        },
        output: to_impl_outcome,
        ..input.sig.clone()
    };

    let constness = input.sig.constness;
    let asyncness = input.sig.asyncness;

    let mut closure_block = input.block;
    ReplaceTry.visit_block_mut(&mut closure_block);

    let mut closure: ExprClosure = parse_quote! {
        #constness #asyncness move |_unsafe_iex_marker| -> #result_type {
            let _iex_no_copy = _iex_no_copy; // Force FnOnce inference
            #closure_block
        }
    };

    closure.attrs = input
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("doc") && !attr.path().is_ident("inline"))
        .cloned()
        .collect();
    let inline_attr = input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("inline"));
    closure.attrs.insert(0, parse_quote! { #[inline(always)] });

    let name = input.sig.ident.clone();

    let wrapper_fn = ItemFn {
        attrs: vec![
            parse_quote! { #[cfg(not(doc))] },
            parse_quote! { #[::iex::imp::fix_hidden_lifetime_bug] },
            parse_quote! { #[inline(always)] },
        ],
        vis: input.vis.clone(),
        sig: wrapper_sig,
        block: parse_quote_spanned! {
            // This span is required for dead code diagnostic
            input_span =>
            {
                let _iex_no_copy = ::iex::imp::NoCopy; // Force FnOnce inference
                // We need { .. } to support the #[inline] attribute on the closure
                #[allow(unused_mut)]
                let mut #name = { #closure };
                ::iex::imp::IexResult::new(
                    #inline_attr move |_unsafe_iex_marker| {
                        ::iex::Outcome::get_value_or_panic(
                            #name(_unsafe_iex_marker),
                            _unsafe_iex_marker,
                        )
                    },
                )
            }
        },
    };

    let doc = "
    <span></span>

    <style>
        .item-decl code::before {
            display: block;
            content: '#[iex]';
        }
    </style>
    ";
    let mut doc_attrs = input.attrs;
    doc_attrs.insert(0, parse_quote! { #[cfg(doc)] });
    doc_attrs.push(parse_quote! { #[doc = #doc] });
    let doc_fn = ItemFn {
        attrs: doc_attrs,
        vis: input.vis,
        sig: input.sig,
        block: parse_quote! {{}},
    };

    quote! {
        #wrapper_fn
        #doc_fn
    }
    .into()
}