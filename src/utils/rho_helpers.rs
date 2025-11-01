pub fn change_contract_token_name(rho_code: &str, new_token_name: &str) -> String {
    println!("🔍 Changing rho code to new token: {}", new_token_name);
    let contract_code = rho_code
        .replace("ASI", &new_token_name.to_uppercase())
        .replace("asi", &new_token_name.to_lowercase());

    //println!("🔍 Rho code with new token: {}", contract_code);
    contract_code
}
