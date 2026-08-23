// esp-idf-sys builds ESP-IDF itself and hands the resulting link flags to Cargo
// through embuild; without this the link step has no IDF to link against.
fn main() {
    embuild::espidf::sysenv::output();
}
