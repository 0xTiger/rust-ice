import os
from contextlib import contextmanager

from sqlalchemy import create_engine, Integer, Column, String, JSON, BigInteger, Float, DateTime, Boolean
from sqlalchemy.orm import sessionmaker
from sqlalchemy.ext.declarative import declarative_base

Base = declarative_base()
metadata = Base.metadata

with open('.env') as f:
    for line in f:
        if line.startswith('#') or not line.strip():
            continue
        key, value = line.strip().split('=', 1)
        os.environ[key] = value

POSTGRES_USER = os.getenv('POSTGRES_USER')
POSTGRES_PASSWORD = os.getenv('POSTGRES_PASSWORD')
POSTGRES_HOST = 'localhost' # os.getenv('POSTGRES_HOST')
SUPERMARKET_DB = os.getenv('SUPERMARKET_DB')

engine = create_engine(f"postgresql://{POSTGRES_USER}:{POSTGRES_PASSWORD}@{POSTGRES_HOST}:5444/{SUPERMARKET_DB}")


@contextmanager
def db_ctx():
    local_session_func = sessionmaker(autocommit=False, autoflush=False, bind=engine)
    db = local_session_func()
    try:
        yield db
    except Exception as e:
        db.rollback()
        raise e
    finally:
        db.close()


class Product(Base):
    __tablename__ = 'product'

    id = Column(Integer, primary_key=True)
    gtin = Column(Integer, index=True)
    json_ld = Column(JSON)
    name = Column(String)
    sku = Column(BigInteger)
    image = Column(String)
    description = Column(String)
    rating = Column(Float)
    review_count = Column(Integer)
    brand = Column(String)
    price = Column(Float)
    url = Column(String)
    availability = Column(String)
    scraped = Column(DateTime)
    seller = Column(String)


class ProductScrapeStatus(Base):
    __tablename__ = 'productscrapestatus'

    id = Column(Integer, primary_key=True)
    url = Column(String, unique=True)
    scrape_success = Column(Boolean)
    fail_reason = Column(String)
    last_scraped = Column(DateTime)
    seller = Column(String)