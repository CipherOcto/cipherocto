package com.cablescan;

import android.Manifest;
import android.app.Activity;
import android.bluetooth.BluetoothAdapter;
import android.bluetooth.BluetoothDevice;
import android.bluetooth.BluetoothManager;
import android.content.Context;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.Arrays;

import javax.crypto.Cipher;
import javax.crypto.Mac;
import javax.crypto.spec.IvParameterSpec;
import javax.crypto.spec.SecretKeySpec;

public class MainActivity extends Activity {
    private static final String TAG = "CableScan";
    private static final int REQ_BT_SCAN = 1;

    /** The secret the desktop smoke test uses to derive the EidKey.
     *  Both ends must agree. Mirrors the smoke's
     *  `secret = [0x42u8; 16]`. */
    private static final byte[] TEST_SECRET = new byte[16];
    static {
        Arrays.fill(TEST_SECRET, (byte) 0x42);
    }

    /** Pre-derived EidKey from TEST_SECRET. Cached so we don't
     *  HKDF on every callback. */
    private static byte[] EID_KEY;  // 64 bytes: AES(32) | HMAC(32)

    private BluetoothAdapter adapter;
    private boolean scanning = false;
    private long startMillis = 0L;

    private final BluetoothAdapter.LeScanCallback cb = new BluetoothAdapter.LeScanCallback() {
        @Override
        public void onLeScan(BluetoothDevice device, int rssi, byte[] scanRecord) {
            long t = System.currentTimeMillis() - startMillis;
            StringBuilder sb = new StringBuilder(256);
            sb.append("t=").append(t).append("ms ");
            if (device != null) {
                sb.append("mac=").append(device.getAddress()).append(' ');
                String name;
                try { name = device.getName(); } catch (SecurityException se) { name = "<denied>"; }
                sb.append("name=").append(name == null ? "<null>" : name).append(' ');
            }
            sb.append("rssi=").append(rssi).append(' ');
            if (scanRecord != null) {
                sb.append("len=").append(scanRecord.length).append(' ');
                byte[] svc_data = extractSvcData(scanRecord, (short) 0xfff9);
                if (svc_data != null) {
                    sb.append("svc_data=").append(hex(svc_data)).append(' ');
                    if (svc_data.length == 20 && EID_KEY != null) {
                        byte[] eid = decryptAdvert(svc_data, EID_KEY);
                        if (eid != null) {
                            sb.append("DECRYPTED_EID=").append(hex(eid)).append(' ');
                        } else {
                            sb.append("DECRYPT_FAILED(hmac_mismatch) ");
                        }
                    } else if (svc_data.length != 20) {
                        sb.append("svc_data_len=").append(svc_data.length).append(" (not 20) ");
                    }
                }
            }
            Log.i(TAG, sb.toString());
        }
    };

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Log.i(TAG, "=== CableScan starting (caBLE v2 round-trip) ===");
        startMillis = System.currentTimeMillis();

        // Pre-compute EidKey
        try {
            EID_KEY = deriveEidKey(TEST_SECRET);
            Log.i(TAG, "EID_KEY[0..8]=" + hex(Arrays.copyOf(EID_KEY, 8))
                + " [32..40]=" + hex(Arrays.copyOfRange(EID_KEY, 32, 40)));
        } catch (Exception e) {
            Log.e(TAG, "HKDF derive failed: " + e);
            finish();
            return;
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            if (checkSelfPermission(Manifest.permission.BLUETOOTH_SCAN)
                    != PackageManager.PERMISSION_GRANTED) {
                Log.w(TAG, "BLUETOOTH_SCAN not granted — requesting");
                requestPermissions(
                    new String[] { Manifest.permission.BLUETOOTH_SCAN },
                    REQ_BT_SCAN);
                return;
            }
        }
        startScanning();
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions,
                                          int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode == REQ_BT_SCAN) {
            int granted = (grantResults.length > 0)
                ? grantResults[0] : PackageManager.PERMISSION_DENIED;
            Log.i(TAG, "BLUETOOTH_SCAN grant result = " + granted);
            if (granted == PackageManager.PERMISSION_GRANTED) {
                startScanning();
            } else {
                Log.e(TAG, "BLUETOOTH_SCAN denied; cannot scan");
                finish();
            }
        }
    }

    private void startScanning() {
        BluetoothManager mgr = (BluetoothManager) getSystemService(Context.BLUETOOTH_SERVICE);
        if (mgr == null) { Log.e(TAG, "no BluetoothManager"); finish(); return; }
        adapter = mgr.getAdapter();
        if (adapter == null || !adapter.isEnabled()) {
            Log.e(TAG, "adapter=" + adapter); finish(); return;
        }
        Log.i(TAG, "adapter ok addr=" + adapter.getAddress());
        boolean ok = adapter.startLeScan(cb);
        scanning = ok;
        Log.i(TAG, "startLeScan ok=" + ok);
        new Handler(Looper.getMainLooper()).postDelayed(new Runnable() {
            @Override public void run() { stopAndFinish(); }
        }, 120_000L);
    }

    private void stopAndFinish() {
        if (scanning && adapter != null) {
            try { adapter.stopLeScan(cb); } catch (Throwable t) { Log.w(TAG, "stop: " + t); }
            scanning = false;
        }
        Log.i(TAG, "=== CableScan done ===");
        finish();
    }

    // ── AD parsing ─────────────────────────────────────────────────

    /** Find Service Data AD (types 0x16, 0x20, 0x21, 0x22) for the
     *  given 16-bit service UUID. Returns payload after the UUID
     *  bytes (i.e. just the data), or null if not found. */
    private static byte[] extractSvcData(byte[] rec, short uuid16) {
        int i = 0;
        while (i < rec.length) {
            int len = rec[i] & 0xff;
            if (len == 0) break;
            if (i + 1 + len > rec.length) break;
            int type = rec[i + 1] & 0xff;
            byte[] payload = new byte[len - 1];
            System.arraycopy(rec, i + 2, payload, 0, len - 1);
            int uuidStart = -1;
            switch (type) {
                case 0x16: case 0x20: uuidStart = 0; break;
                case 0x21: case 0x22: uuidStart = 2; break;
            }
            if (uuidStart >= 0 && payload.length >= uuidStart + 2) {
                int got = ((payload[uuidStart] & 0xff)
                         | ((payload[uuidStart + 1] & 0xff) << 8));
                if (got == (uuid16 & 0xffff)) {
                    int dataFrom = uuidStart + 2;
                    return Arrays.copyOfRange(payload, dataFrom, payload.length);
                }
            }
            i += 1 + len;
        }
        return null;
    }

    // ── caBLE v2 decryption (HKDF + HMAC + AES-128-CTR) ─────────

    /** Derive 64-byte EidKey from `secret` per Chromium's
     *  Discovery::DerivedValueType::EidKey. */
    private static byte[] deriveEidKey(byte[] secret) throws Exception {
        // info = 4-byte little-endian of EidKey enum tag (1)
        byte[] info = new byte[] { 0x01, 0x00, 0x00, 0x00 };
        return hkdfSha256(new byte[0], secret, info, 64);
    }

    /** HKDF-SHA256 (RFC 5869). Returns `length` bytes of OKM. */
    private static byte[] hkdfSha256(byte[] salt, byte[] ikm, byte[] info, int length)
            throws Exception {
        // Extract
        Mac mac = Mac.getInstance("HmacSHA256");
        mac.init(new SecretKeySpec(salt.length == 0 ? new byte[32] : salt, "HmacSHA256"));
        byte[] prk = mac.doFinal(ikm);
        // Expand
        byte[] okm = new byte[length];
        byte[] t = new byte[0];
        int pos = 0;
        byte counter = 1;
        while (pos < length) {
            mac.init(new SecretKeySpec(prk, "HmacSHA256"));
            mac.update(t);
            mac.update(info);
            mac.update(counter);
            t = mac.doFinal();
            int copy = Math.min(t.length, length - pos);
            System.arraycopy(t, 0, okm, pos, copy);
            pos += copy;
            counter++;
        }
        return okm;
    }

    /** Decrypt a 20-byte caBLE advert: HMAC verify + AES-128-CTR.
     *  Returns the 16-byte Eid, or null if HMAC doesn't match. */
    private static byte[] decryptAdvert(byte[] advert, byte[] eidKey) {
        try {
            byte[] ciphertext = Arrays.copyOf(advert, 16);
            byte[] receivedTag = Arrays.copyOfRange(advert, 16, 20);

            // 1. HMAC verify
            Mac mac = Mac.getInstance("HmacSHA256");
            mac.init(new SecretKeySpec(
                Arrays.copyOfRange(eidKey, 32, 64), "HmacSHA256"));
            mac.update(ciphertext);
            byte[] computed = mac.doFinal();
            // Compare first 4 bytes constant-time
            int diff = 0;
            for (int i = 0; i < 4; i++) diff |= receivedTag[i] ^ computed[i];
            if (diff != 0) return null;

            // 2. AES-128-CTR decrypt
            Cipher c = Cipher.getInstance("AES/CTR/NoPadding");
            c.init(Cipher.DECRYPT_MODE,
                new SecretKeySpec(Arrays.copyOf(eidKey, 16), "AES"),
                new IvParameterSpec(new byte[16]));
            return c.doFinal(ciphertext);
        } catch (Exception e) {
            Log.w(TAG, "decrypt err: " + e);
            return null;
        }
    }

    // ── utilities ──────────────────────────────────────────────────

    private static String hex(byte[] b) {
        StringBuilder s = new StringBuilder(b.length * 2);
        for (byte x : b) {
            String h = Integer.toHexString(x & 0xff);
            if (h.length() < 2) s.append('0');
            s.append(h);
        }
        return s.toString();
    }
}