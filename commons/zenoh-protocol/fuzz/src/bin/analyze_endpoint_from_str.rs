use zenoh_protocol_fuzz::analyze_endpoint_from_str;

fn main() {
    let arg = std::env::args()
        .nth(1)
        .expect("usage: analyze_endpoint_from_str <endpoint-string>");
    let analysis = analyze_endpoint_from_str(arg.as_bytes());

    println!("input_len: {}", analysis.input_len);
    println!("input: {:?}", analysis.input);
    println!("decode_ok: {}", analysis.decoded.is_some());
    println!("roundtrip_ok: {}", analysis.roundtrip_ok);
    if let Some(endpoint) = analysis.decoded {
        println!("decoded_endpoint: {endpoint:#?}");
    }
}
