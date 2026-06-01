import re

with open('contracts/amm/src/lib.rs', 'r') as f:
    code = f.read()

# Fix add_liquidity(provider, amount_a, amount_b, min_shares, deadline) -> add_liquidity(..., 0, 0, min_shares, deadline)
code = re.sub(
    r'\.add_liquidity\(\s*(&[^,]+),\s*(&[^,]+),\s*(&[^,]+),\s*(&[^,]+),\s*(&[^,]+)\s*\)',
    r'.add_liquidity(\1, \2, \3, &0_i128, &0_i128, \4, \5)',
    code
)

# Fix swap(trader, token_in, amount_in, min_out, deadline) -> swap(..., &None)
code = re.sub(
    r'\.swap\(\s*(&[^,]+),\s*(&[^,]+),\s*(&[^,]+),\s*(&[^,]+),\s*(&[^,]+)\s*\)',
    r'.swap(\1, \2, \3, \4, \5, &None)',
    code
)

# Fix swap_exact_out(trader, token_out, amount_out, max_in, deadline) -> swap_exact_out(..., &None)
code = re.sub(
    r'\.swap_exact_out\(\s*(&[^,]+),\s*(&[^,]+),\s*(&[^,]+),\s*(&[^,]+),\s*(&[^,]+)\s*\)',
    r'.swap_exact_out(\1, \2, \3, \4, \5, &None)',
    code
)

with open('contracts/amm/src/lib.rs', 'w') as f:
    f.write(code)
