module Auth.Crypto.Password
  ( PasswordParams (..)
  , StoredPassword (..)
  , hashNewPassword
  , verifyPassword
  ) where

import           Crypto.Hash.Algorithms (SHA256 (..))
import           Crypto.KDF.PBKDF2      (Parameters (..), generate, prfHMAC)
import           Crypto.MAC.HMAC        (HMAC, hmac)
import           Crypto.Random          (getRandomBytes)
import           Data.ByteArray         (constEq, convert)
import qualified Data.ByteString        as BS
import           Data.Text              (Text)
import           Data.Text.Encoding     (encodeUtf8)

data PasswordParams = PasswordParams
  { ppPepper     :: BS.ByteString
  , ppIterations :: Int
  }

data StoredPassword = StoredPassword
  { spSalt :: BS.ByteString
  , spHash :: BS.ByteString
  } deriving (Show, Eq)

saltLength, keyLength :: Int
saltLength = 16
keyLength  = 32

peppered :: BS.ByteString -> Text -> BS.ByteString
peppered pepper password =
  convert (hmac pepper (encodeUtf8 password) :: HMAC SHA256)

derive :: PasswordParams -> BS.ByteString -> BS.ByteString -> BS.ByteString
derive PasswordParams{..} salt pwInput =
  generate (prfHMAC SHA256) (Parameters ppIterations keyLength) pwInput salt

hashNewPassword :: PasswordParams -> Text -> IO StoredPassword
hashNewPassword params password = do
  salt <- getRandomBytes saltLength
  let pwInput = peppered (ppPepper params) password
  pure StoredPassword { spSalt = salt, spHash = derive params salt pwInput }

verifyPassword :: PasswordParams -> Text -> StoredPassword -> Bool
verifyPassword params password StoredPassword{..} =
  let pwInput   = peppered (ppPepper params) password
      candidate = derive params spSalt pwInput
  in constEq candidate spHash
