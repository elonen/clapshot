"""Remove redundant metadata field

Revision ID: 118e820845d5
Revises: 5b50ae9ea790
Create Date: 2022-09-30 00:56:20.920145

"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = '118e820845d5'
down_revision = '5b50ae9ea790'
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.drop_column('video', 'raw_metadata_video')


def downgrade() -> None:
    op.add_column('video', sa.Column('raw_metadata_video', sa.VARCHAR(), nullable=True))
