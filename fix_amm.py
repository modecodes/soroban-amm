import re

with open('contracts/amm/src/lib.rs', 'r') as f:
    code = f.read()

code = code.replace('test_price_ratio(, &None)', 'test_price_ratio()')
code = code.replace('amm.get_info(, &None)', 'amm.get_info()')
code = code.replace('setup_pool(30, &None)', 'setup_pool(30)')
code = code.replace('symbol_short!("swap", &None)', 'symbol_short!("swap")')
code = code.replace('price (1_000_000, &None)', 'price (1_000_000)')
code = code.replace('/ (1_000_000 + amount_in, &None)', '/ (1_000_000 + amount_in)')
code = code.replace('Self::get_reserve_a(env.clone(), &None)', 'Self::get_reserve_a(env.clone())')

with open('contracts/amm/src/lib.rs', 'w') as f:
    f.write(code)
