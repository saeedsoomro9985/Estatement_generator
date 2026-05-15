import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from generator import _render_pdf

data = {
    'id': 'T', 'name': 'Ibrahim Zoaib', 'address': 'Karachi, Pakistan',
    'period': {'from': '01-Jan-2024', 'to': '31-Mar-2024'},
    'accounts': [{
        'accountNumber': 'PK36HABB000000001234', 'accountType': 'CURRENT',
        'balance': 1250000.0,
        'transactions': [
            {'date': f'2024-01-{i+1:02d}', 'description': f'Transaction {i+1}',
             'amount': 1000.0, 'balance': 1250000.0 - i*1000}
            for i in range(25)
        ],
    }],
    'tdr': [{
        'tdrNumber': 'TD-000001', 'principalAmount': 500000.0,
        'interestRate': 7.5, 'maturityDate': '2025-01-01',
        'transactions': [{
            'date': '2024-01-01', 'description': 'Fixed Deposit 1Y Monthly Profit',
            'amount': 500000.0, 'balance': 500000.0,
        }],
    }],
}

os.makedirs('output', exist_ok=True)
b = _render_pdf(data)
with open('output/SAMPLE-avanza.pdf', 'wb') as f:
    f.write(b)
print(f'Python OK  {len(b):,} bytes')
